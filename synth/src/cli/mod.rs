#![allow(clippy::assertions_on_result_states)]
mod csv;
mod export;
mod import;
mod import_utils;
mod json;
mod jsonl;
mod mongo;
mod mysql;
mod postgres;
mod store;

use crate::cli::import::ImportStrategy;
use crate::cli::store::Store;
use crate::sampler::Sampler;
use crate::version::print_version_message;

use anyhow::{Context, Result};
use rand::RngCore;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use structopt::clap::AppSettings;
use structopt::StructOpt;
use synth_core::DataSourceParams;
use uriparse::URI;

pub(crate) mod config;

#[cfg(feature = "telemetry")]
use std::cell::RefCell;
#[cfg(feature = "telemetry")]
use std::rc::Rc;
#[cfg(feature = "telemetry")]
pub mod telemetry;

#[cfg(feature = "telemetry")]
use telemetry::{TelemetryContext, TelemetryExportStrategy};

use self::export::ExportStrategyBuilder;

pub struct Cli {
    store: Store,
    #[cfg(feature = "telemetry")]
    telemetry_context: Rc<RefCell<TelemetryContext>>,
}

impl<'w> Cli {
    pub fn new() -> Result<Self> {
        #[cfg(debug_assertions)]
        {
            let splash = crate::utils::splash::Splash::auto()?;
            log::debug!("{}", splash);
        }

        Ok(Self {
            store: Store::init()?,
            #[cfg(feature = "telemetry")]
            telemetry_context: Rc::new(RefCell::new(TelemetryContext::new())),
        })
    }

    #[cfg(feature = "telemetry")]
    pub fn get_telemetry_context(&self) -> TelemetryContext {
        self.telemetry_context.borrow().clone()
    }

    fn derive_seed(random: bool, seed: Option<u64>) -> Result<u64> {
        if random && seed.is_some() {
            return Err(anyhow!(
                "Cannot have the --random flag and --seed specified at the same time."
            ));
        }
        match random {
            true => Ok(rand::thread_rng().next_u64()),
            false => Ok(seed.unwrap_or(0)),
        }
    }

    pub async fn run<W: Write + 'w>(&self, args: Args, writer: W) -> Result<()> {
        match args {
            Args::Generate(cmd) => self.generate(cmd, writer),
            Args::Import(cmd) => self.import(cmd),
            #[cfg(feature = "telemetry")]
            Args::Telemetry(cmd) => self.telemetry(cmd, writer),
            Args::Version => {
                print_version_message(writer);
                Ok(())
            }
        }
    }

    #[cfg(feature = "telemetry")]
    fn telemetry<W: Write>(&self, cmd: TelemetryCommand, mut writer: W) -> Result<()> {
        match cmd {
            TelemetryCommand::Enable => telemetry::enable(),
            TelemetryCommand::Disable => telemetry::disable(),
            TelemetryCommand::Status => {
                if telemetry::is_enabled() {
                    writeln!(
                        writer,
                        "Telemetry is enabled. To disable it run `synth telemetry disable`."
                    )
                    .expect("failed to write telemetry status");
                } else {
                    writeln!(
                        writer,
                        "Telemetry is disabled. To enable it run `synth telemetry enable`."
                    )
                    .expect("failed to write telemetry status");
                }
                Ok(())
            }
        }
    }

    fn import(&self, cmd: ImportCommand) -> Result<()> {
        // TODO: If ns exists and no collection: break
        // If collection and ns exists and collection exists: break

        let import_strategy: Box<dyn ImportStrategy> = DataSourceParams {
            uri: URI::try_from(cmd.from.as_str())
                .with_context(|| format!("Parsing import URI '{}'", cmd.from))?,
            schema: cmd.schema,
            concurrency: 1,
        }
        .try_into()?;

        if let Some(collection) = cmd.collection {
            if self.store.collection_exists(&cmd.namespace, &collection) {
                Err(anyhow!(
                    "The collection `{}` already exists. Will not import into an existing collection.",
                    Store::relative_collection_path(&cmd.namespace, &collection).display()
                ))
            } else {
                let content = import_strategy.import_collection(&collection)?;
                self.store
                    .save_collection_path(&cmd.namespace, collection, content)?;

                #[cfg(feature = "telemetry")]
                self.telemetry_context.borrow_mut().set_num_collections(1);

                Ok(())
            }
        } else if self.store.ns_exists(&cmd.namespace) {
            Err(anyhow!(
                "The directory at `{}` already exists. Will not import into an existing directory.",
                cmd.namespace.display()
            ))
        } else {
            let ns = import_strategy.import()?;

            #[cfg(feature = "telemetry")]
            TelemetryExportStrategy::fill_telemetry(
                Rc::clone(&self.telemetry_context),
                &ns,
                cmd.collection,
                cmd.namespace.clone(),
            )?;

            self.store.save_ns_path(cmd.namespace, ns)?;

            Ok(())
        }
    }

    fn generate<W: Write + 'w>(&self, cmd: GenerateCommand, writer: W) -> Result<()> {
        let mut namespace = self.store.get_ns(cmd.namespace.clone()).context(format!(
            "Unable to open the namespace \"{}\"",
            cmd.namespace
                .to_str()
                .expect("The provided namespace is not a valid UTF-8 string")
        ))?;

        let builder: ExportStrategyBuilder<_> = DataSourceParams {
            uri: URI::try_from(cmd.to.as_str())
                .with_context(|| format!("Parsing generation URI '{}'", cmd.to))?,
            schema: cmd.schema,
            concurrency: cmd.concurrency,
        }
        .try_into()?;

        let builder = builder.set_writer(writer);

        // `mut` is only used by the "telemetry" feature
        #[allow(unused_mut)]
        let mut export_strategy = builder.build()?;

        #[cfg(feature = "telemetry")]
        {
            export_strategy = Box::new(TelemetryExportStrategy::new(
                export_strategy,
                Rc::clone(&self.telemetry_context),
                cmd.collection.clone(),
                cmd.namespace.clone(),
            ));
        }

        // Change namespace if scenario
        if let Some(scenario) = cmd.scenario {
            let scenario = self
                .store
                .get_scenario(namespace, cmd.namespace.clone(), &scenario)?;

            namespace = scenario.build()?;
        }

        let seed = Self::derive_seed(cmd.random, cmd.seed)?;
        let sample =
            Sampler::try_from(&namespace)?.sample_seeded(cmd.collection.clone(), cmd.size, seed)?;

        export_strategy
            .export(namespace, sample)
            .with_context(|| format!("At namespace {:?}", cmd.namespace))?;

        Ok(())
    }
}

// The serialization of this enum is used for telemetry when synth panics and we want our logs to
// contain the command that caused the panic. When modifying this, pay attention to skip
// serialization of any privacy sensitive information.
#[derive(StructOpt, Serialize)]
#[structopt(
    name = "synth",
    about = "synthetic data engine on the command line",
    no_version,
    global_settings = &[AppSettings::DisableVersion])]
pub enum Args {
    #[structopt(about = "Generate data from a namespace", alias = "gen")]
    Generate(GenerateCommand),
    #[structopt(about = "Import data from an external source")]
    Import(ImportCommand),
    #[cfg(feature = "telemetry")]
    #[structopt(about = "Toggle anonymous usage data collection")]
    Telemetry(TelemetryCommand),
    #[structopt(about = "Version information")]
    Version,
}

fn map_from_uri_query<'a>(query_opt: Option<&'a uriparse::Query<'a>>) -> HashMap<&'a str, &'a str> {
    let query_str = query_opt.map(uriparse::Query::as_str).unwrap_or_default();

    HashMap::<_, _>::from_iter(querystring::querify(query_str))
}

#[derive(StructOpt, Serialize)]
pub struct GenerateCommand {
    #[structopt(
        help = "The namespace directory from which to read schema files",
        parse(from_os_str)
    )]
    #[serde(skip)]
    pub namespace: PathBuf,
    #[structopt(
        long,
        help = "The specific collection from which to generate. Cannot be used with --scenario"
    )]
    #[serde(skip)]
    pub collection: Option<String>,
    #[structopt(
        long,
        help = "The specific scenario to generate data for. Cannot be used with --collection",
        conflicts_with("collection")
    )]
    #[serde(skip)]
    pub scenario: Option<String>,
    #[structopt(long, help = "the number of samples", default_value = "1")]
    pub size: usize,
    #[structopt(
        long,
        help = "The URI into which data will be generated. Can be a file-based URI scheme to output data to the filesystem or stdout ('json:', 'jsonl:' and 'csv:' allow outputting JSON, JSON Lines and CSV data respectively) or can be a database URI to write data directly to some database (supports Postgres, MongoDB, and MySQL). Defaults to writing JSON data to stdout. [example: jsonl:/tmp/generation_output]",
        default_value = "json:"
    )]
    #[serde(skip)]
    pub to: String,
    #[structopt(
        long,
        help = "an unsigned 64 bit integer seed to be used as a seed for generation"
    )]
    pub seed: Option<u64>,
    #[structopt(
        long,
        help = "generation will use a random seed - this cannot be used with --seed"
    )]
    pub random: bool,
    #[structopt(
        long,
        help = "(Postgres only) Specify the schema into which to generate. Defaults to 'public'."
    )]
    #[serde(skip)]
    pub schema: Option<String>,
    #[structopt(
        long,
        help = "The maximum number of concurrent tasks writing to the database.",
        default_value = "3"
    )]
    #[serde(skip)]
    pub concurrency: usize,
}

#[derive(StructOpt, Serialize)]
pub struct ImportCommand {
    #[structopt(
        help = "The namespace directory into which to save imported schema files",
        parse(from_os_str)
    )]
    #[serde(skip)]
    pub namespace: PathBuf,
    #[structopt(
        long,
        help = "The name of a collection into which the data will be imported"
    )]
    #[serde(skip)]
    pub collection: Option<String>,
    #[structopt(
        long,
        help = "The source URI from which to import data. Can be a file-based URI scheme to read data from a file or stdin ('json:', 'jsonl:' and 'csv:' allow reading JSON, JSON Lines and CSV data respectively) or can be a database URI to read data directly from some database (supports Postgres, MongoDB, and MySQL). Defaults to reading JSON data from stdin. [example: jsonl:/tmp/test_data_input]",
        default_value = "json:"
    )]
    #[serde(skip)]
    pub from: String,
    #[structopt(
        long,
        help = "(Postgres only) Specify the schema from which to import. Defaults to 'public'."
    )]
    #[serde(skip)]
    pub schema: Option<String>,
}

#[cfg(feature = "telemetry")]
#[derive(StructOpt, Serialize)]
pub enum TelemetryCommand {
    #[structopt(about = "Enable anonymous usage data collection")]
    Enable,
    #[structopt(about = "Disable anonymous usage data collection")]
    Disable,
    #[structopt(about = "Check telemetry status")]
    Status,
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_derive_seed() {
        assert_eq!(Cli::derive_seed(false, None).unwrap(), 0);
        assert_eq!(Cli::derive_seed(false, Some(5)).unwrap(), 5);
        assert!(Cli::derive_seed(true, Some(5)).is_err());
        assert!(Cli::derive_seed(true, None).is_ok());
    }
}
