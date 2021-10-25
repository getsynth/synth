use anyhow::{Context, Result};
use backtrace::Backtrace;
use colored::Colorize;
use lazy_static::lazy_static;

use std::collections::HashMap;
use std::error::Error;
use std::future::Future;
use std::io::{self, BufRead, Read, Write};
use std::panic;
use uuid::Uuid;

use crate::cli::config;
use crate::utils::META_OS;
use crate::version::version;

use synth_core::{
    compile::{Address, CompilerState, FromLink, Source},
    Compile, Compiler, Content, Graph, Namespace,
};

use super::{Args, TelemetryCommand};

const API_KEY: &str = "L-AQtrFVtZGL_PjK2FbFLBR3oXNtfv8OrCD8ObyeBQo";
const EVENT_NAME: &str = "synth-command";

lazy_static! {
    static ref TELEMETRY_CLIENT: TelemetryClient = TelemetryClient::new();
}

fn send_panic_report(synth_command: &str, telemetry_client: &TelemetryClient) {
    // This exists only to return early instead of panicking inside this panic hook
    let send_panic_report_impl = || -> Result<()> {
        let (stdin, stderr) = (io::stdin(), io::stderr());
        let (mut stdin, mut stderr) = (stdin.lock(), stderr.lock());
        let mut username = None;
        let mut email = None;

        eprintln!("\n{}", "Synth encountered a fatal error. Because telemetry is enabled, an anonymous \
            bug report will be sent to the Synth developers so that we may investigate the problem.".red());

        // Only interact with the user if there is an actual user running Synth; if the program's
        // stderr was redirected or this is running as part of CI, skip the interaction
        if console::user_attended_stderr() {
            eprint!("\nWould you also like to send your name and e-mail? One of our developers may enter \
                in contact to obtain further information if deemed necessary.\nAnswer (y/n): ");
            stderr.flush()?;

            // Newline character is also stored
            let mut answer = [0; 2];
            stdin
                .read_exact(&mut answer)
                .context("Couldn't read answer.")?;

            if answer[0] == b'y' {
                eprint!("Name: ");
                stderr.flush()?;

                let mut n = String::new();
                stdin.read_line(&mut n).context("Couldn't read name.")?;
                username = Some(n.trim().to_owned());

                eprint!("E-mail: ");
                stderr.flush()?;

                let mut e = String::new();
                stdin.read_line(&mut e).context("Couldn't read e-mail.")?;
                email = Some(e.trim().to_owned());
            }
        }

        let backtrace = Backtrace::new();
        let panic_report = PanicReport::new(username, email, synth_command.to_owned(), backtrace);

        telemetry_client.send_panic_report(panic_report)?;

        Ok(())
    };

    match send_panic_report_impl() {
        Ok(()) => eprintln!("Bug report sent with success. Thanks for your patience!"),
        Err(e) => eprintln!("Error sending bug report: {}", e),
    }
}

pub(crate) fn enable() -> Result<()> {
    // Initialise the `uuid` if it hasn't been initialised yet.
    let _ = get_or_initialise_uuid();
    Ok(config::set_telemetry_enabled(true))
}

pub(crate) fn disable() -> Result<()> {
    Ok(config::set_telemetry_enabled(false))
}

pub(crate) fn is_enabled() -> bool {
    config::get_telemetry_enabled().unwrap_or(false)
}

fn get_or_initialise_uuid() -> String {
    if config::get_uuid().is_none() {
        config::set_uuid(Uuid::new_v4().to_hyphenated().to_string());
    }
    config::get_uuid().expect("is ok here as was set earlier")
}

#[derive(Clone)]
pub struct TelemetryContext {
    generators: Vec<String>,
}

impl TelemetryContext {
    pub fn new() -> Self {
        TelemetryContext {
            generators: Vec::new(),
        }
    }

    pub fn from_namespace(&mut self, namespace: &Namespace) -> Result<()> {
        let crawler = TelemetryCrawler {
            state: &mut CompilerState::namespace(namespace),
            position: Address::new_root(),
            context: self,
        };

        namespace.compile(crawler)?;

        Ok(())
    }

    pub fn add_generator(&mut self, name: String) {
        self.generators.push(name);
    }
}

pub(self) struct TelemetryCrawler<'t, 'a> {
    state: &'t mut CompilerState<'a, Graph>,
    position: Address,
    context: &'t mut TelemetryContext,
}

impl<'t, 'a: 't> TelemetryCrawler<'t, 'a> {
    fn as_at(&mut self, field: &str, content: &'a Content) -> TelemetryCrawler<'_, 'a> {
        let position = self.position.clone().into_at(field);
        TelemetryCrawler {
            state: self.state.entry(field).or_init(content),
            position,
            context: self.context,
        }
    }

    fn compile(self) -> Result<()> {
        match self.state.source() {
            Source::Namespace(namespace) => namespace.compile(self)?,
            Source::Content(content) => content.compile(self)?,
        };
        Ok(())
    }
}

impl<'t, 'a: 't> Compiler<'a> for TelemetryCrawler<'t, 'a> {
    fn build(&mut self, field: &str, content: &'a Content) -> Result<Graph> {
        self.context.add_generator(format!("{}", content));

        if let Err(err) = self.as_at(field, content).compile() {
            warn!(
                "could not crawl into field `{}` at `{}`",
                field, self.position
            );
            return Err(err);
        }
        Ok(Graph::dummy())
    }

    fn get<S: Into<Address>>(&mut self, _target: S) -> Result<Graph> {
        Ok(Graph::dummy())
    }
}

pub async fn with_telemetry<F, Fut, T, E, G>(
    args: Args,
    func: F,
    func_telemetry_context: G,
) -> Result<T, E>
where
    F: FnOnce(Args) -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: AsRef<dyn Error + 'static>,
    G: FnOnce() -> TelemetryContext,
{
    if !is_enabled() {
        return func(args).await;
    }

    let default_panic_hook = panic::take_hook();
    let synth_command = serde_json::to_string(&args).unwrap();

    // Register a custom panic hook that will run the default hook and send a bug report to PostHog
    panic::set_hook(Box::new(move |panic_info| {
        default_panic_hook(panic_info);
        send_panic_report(&synth_command, &TELEMETRY_CLIENT);
    }));

    let command_name = match &args {
        Args::Init { .. } => "init",
        Args::Generate { .. } => "generate",
        Args::Import { .. } => "import",
        Args::Telemetry(TelemetryCommand::Enable) => "telemetry::enable",
        Args::Telemetry(TelemetryCommand::Disable) => "telemetry::disable",
        Args::Telemetry(TelemetryCommand::Status) => "telemetry::status",
        Args::Version => "version",
    };

    func(args)
        .await
        .and_then(|success| {
            TELEMETRY_CLIENT.success(command_name, success, func_telemetry_context())
        })
        .or_else(|err| TELEMETRY_CLIENT.failed(command_name, err))
}

struct PanicReport {
    username: Option<String>,
    email: Option<String>,
    synth_command: String,
    backtrace: Backtrace,
}

impl PanicReport {
    fn new(
        username: Option<String>,
        email: Option<String>,
        synth_command: String,
        backtrace: Backtrace,
    ) -> Self {
        Self {
            username,
            email,
            synth_command,
            backtrace,
        }
    }
}

enum CommandResult {
    Success,
    Failed,
}

impl ToString for CommandResult {
    fn to_string(&self) -> String {
        match self {
            CommandResult::Success => "success".to_string(),
            CommandResult::Failed => "failed".to_string(),
        }
    }
}

pub(crate) struct TelemetryClient {
    ph_client: posthog_rs::Client,
    uuid: String,
    synth_version: String,
    os: String,
}

impl TelemetryClient {
    fn new() -> Self {
        let synth_version = version();
        let os = META_OS.to_string();

        Self {
            ph_client: posthog_rs::client(API_KEY),
            uuid: get_or_initialise_uuid(),
            synth_version,
            os,
        }
    }

    fn default_telemetry_properties(&self) -> HashMap<String, String> {
        let mut prop_map = HashMap::new();
        prop_map.insert("version".to_string(), self.synth_version.clone());
        prop_map.insert("os".to_string(), self.os.clone());

        prop_map
    }

    pub fn success<T, E>(
        &self,
        command_name: &str,
        output: T,
        telemetry_context: TelemetryContext,
    ) -> Result<T, E> {
        let mut prop_map = self.default_telemetry_properties();
        prop_map.insert("command".to_string(), command_name.to_string());
        prop_map.insert("success".to_string(), CommandResult::Success.to_string());

        if telemetry_context.generators.len() > 0 {
            prop_map.insert(
                "generators".to_string(),
                telemetry_context.generators.join(", "),
            );
        }

        self.send(EVENT_NAME.to_string(), prop_map).or_else(|err| {
            info!("failed to push ok of command: {}", err);
            Ok(())
        })?;
        Ok(output)
    }

    pub fn failed<T, E>(&self, command_name: &str, error: E) -> Result<T, E>
    where
        E: AsRef<dyn Error + 'static>,
    {
        let mut prop_map = self.default_telemetry_properties();
        prop_map.insert("command".to_string(), command_name.to_string());
        prop_map.insert("success".to_string(), CommandResult::Failed.to_string());

        self.send(EVENT_NAME.to_string(), prop_map).or_else(|err| {
            info!("failed to push err of command: {}", err);
            Ok(())
        })?;
        Err(error)
    }

    fn send_panic_report(&self, mut panic_report: PanicReport) -> Result<()> {
        panic_report.backtrace.resolve();

        let mut prop_map = self.default_telemetry_properties();
        prop_map.insert(
            "username".to_string(),
            panic_report.username.unwrap_or_default(),
        );
        prop_map.insert("email".to_string(), panic_report.email.unwrap_or_default());
        prop_map.insert("synth_command".to_string(), panic_report.synth_command);
        prop_map.insert(
            "backtrace".to_string(),
            format!("{:?}", panic_report.backtrace),
        );

        self.send(String::from("synth-panic-report"), prop_map)
    }

    fn send(&self, event: String, prop_map: HashMap<String, String>) -> Result<()> {
        let props = posthog_rs::Properties {
            distinct_id: self.uuid.clone(),
            props: prop_map,
        };

        let event = posthog_rs::Event {
            event,
            properties: props,
            timestamp: None,
        };

        if let Err(err) = self.ph_client.capture(event) {
            debug!("Failed to send message to PostHog. Error: {:?}", err);
            return Err(anyhow!("Failed to send message to PostHog."));
        }

        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use super::{Namespace, TelemetryContext};

    macro_rules! schema {
    {
        $($inner:tt)*
    } => {
        serde_json::from_value::<synth_core::schema::Content>(serde_json::json!($($inner)*))
            .expect("could not deserialize value into a schema")
    }
}

    #[test]
    fn telemetry_context_from_namespace_string_generators() {
        let schema: Namespace = schema!({
            "type": "object",
            "username": {
                "type": "string",
                "faker": {
                    "generator": "username"
                }
            },
            "credit_card": {
                "type": "string",
                "faker": {
                    "generator": "credit_card"
                }
            },
            "email": {
                "type": "string",
                "faker": {
                    "generator": "safe_email"
                }
            }
        })
        .into_namespace()
        .unwrap();

        let mut context = TelemetryContext::new();
        context.from_namespace(&schema).unwrap();

        assert_eq!(
            context.generators,
            vec!(
                "string::credit_card",
                "string::safe_email",
                "string::username"
            )
        );
    }

    #[test]
    fn telemetry_context_from_namespace_number_ranges() {
        let schema: Namespace = schema!({
            "type": "object",
            "int_64": {
                "type": "number",
                "subtype": "i64",
                "range": {
                    "low": -5
                }
            },
            "uint_64": {
                "type": "number",
                "subtype": "u64",
                "range": {
                    "high": 5
                }
            },
            "f_64": {
                "type": "number",
                "subtype": "f64",
                "range": {
                    "high": 3.2
                }
            },
            "int_32": {
                "type": "number",
                "subtype": "i32",
                "range": {
                    "low": -13
                }
            },
            "uint_32": {
                "type": "number",
                "subtype": "u32",
                "range": {
                    "high": 23
                }
            },
            "f_32": {
                "type": "number",
                "subtype": "f32",
                "range": {
                    "high": 21.2
                }
            }
        })
        .into_namespace()
        .unwrap();

        let mut context = TelemetryContext::new();
        context.from_namespace(&schema).unwrap();

        assert_eq!(
            context.generators,
            vec!(
                "number::F32::Range",
                "number::F64::Range",
                "number::I32::Range",
                "number::I64::Range",
                "number::U32::Range",
                "number::U64::Range"
            )
        );
    }

    #[test]
    fn telemetry_context_from_namespace_booleans() {
        let schema: Namespace = schema!({
            "type": "object",
            "constant": {
                "type": "bool",
                "constant": true
            },
            "frequency": {
                "type": "bool",
                "frequency": 0.65
            }
        })
        .into_namespace()
        .unwrap();

        let mut context = TelemetryContext::new();
        context.from_namespace(&schema).unwrap();

        assert_eq!(
            context.generators,
            vec!("bool::constant", "bool::frequency")
        );
    }

    #[test]
    fn telemetry_context_from_namespace_series() {
        let schema: Namespace = schema!({
            "type": "object",
            "series": {
                "type": "series",
                "incrementing": {
                    "start": "2021-02-01 09:00:00",
                    "increment": "1m"
                }
            },
            "poisson": {
                "type": "series",
                "poisson": {
                    "start": "2021-02-01 09:00:00",
                    "rate": "1m"
                }
            },
            "cyclic": {
                "type": "series",
                "cyclical": {
                    "start": "2021-02-01 00:00:00",
                    "period": "1d",
                    "min_rate": "10m",
                    "max_rate": "30s"
                }
            }
        })
        .into_namespace()
        .unwrap();

        let mut context = TelemetryContext::new();
        context.from_namespace(&schema).unwrap();

        assert_eq!(
            context.generators,
            vec!(
                "series::cyclical",
                "series::poisson",
                "series::incrementing"
            )
        );
    }
}
