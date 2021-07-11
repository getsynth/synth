mod export;
mod import;
mod mongo;
mod postgres;
mod stdf;
mod store;
mod telemetry;

use crate::cli::export::SomeExportStrategy;
use crate::cli::export::{ExportParams, ExportStrategy};
use crate::cli::import::ImportStrategy;
use crate::cli::import::SomeImportStrategy;
use crate::cli::store::Store;
use anyhow::{Context, Result};

use std::fs::File;
use std::path::{Path, PathBuf};
use structopt::StructOpt;

use crate::cli::telemetry::TelemetryClient;
use rand::RngCore;
use synth_core::Name;

pub struct Cli {
    store: Store,
    args: CliArgs,
    telemetry: TelemetryClient,
}

fn with_telemetry<F: FnOnce() -> Result<T>, T>(
    command: &str,
    tel_client: &TelemetryClient,
    func: F,
) -> Result<T> {
    match func() {
        Ok(t) => {
            let _ = tel_client.success(command);
            Ok(t)
        }
        Err(e) => {
            let _ = tel_client.failed(command);
            Err(e)
        }
    }
}

impl Cli {
    /// this is going to get confusing with `init` command
    pub fn new(args: CliArgs, version: String, os: String) -> Result<Self> {
        Ok(Self {
            store: Store::init()?,
            args,
            telemetry: TelemetryClient::new(version, os),
        })
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

    pub async fn run(self) -> Result<()> {
        match self.args {
            CliArgs::Generate {
                ref namespace,
                ref collection,
                size,
                ref to,
                seed,
                random,
            } => with_telemetry("generate", &self.telemetry, || {
                self.generate(
                    namespace.clone(),
                    collection.clone(),
                    size,
                    to.clone(),
                    Self::derive_seed(random, seed)?,
                )
            }),
            CliArgs::Import {
                ref namespace,
                ref collection,
                ref from,
            } => with_telemetry("import", &self.telemetry, || {
                self.import(namespace.clone(), collection.clone(), from.clone())
            }),
            CliArgs::Init { ref init_path } => with_telemetry("init", &self.telemetry, || self.init(init_path.clone())),
            CliArgs::Telemetry(telemetry) => {
                match telemetry {
                    TelemetryCommand::Enable => {
                        with_telemetry("telemetry::enable", &self.telemetry, telemetry::enable)
                    }
                    TelemetryCommand::Disable => {
                        with_telemetry("telemetry::disable", &self.telemetry, || {
                            telemetry::disable()
                        })
                    }
                    TelemetryCommand::Status => {
                        if telemetry::is_enabled() {
                            println!("Telemetry is enabled. To disable it run `synth telemetry disable`.");
                        } else {
                            println!(
                                "Telemetry is disabled. To enable it run `synth telemetry enable`."
                            );
                        }
                        Ok(())
                    }
                }
            }
        }
    }

    fn init(&self, init_path: Option<PathBuf>) -> Result<()> {
        let base_path = match init_path {
            Some(path) => std::fs::canonicalize(".")?.join(path),
            None => std::fs::canonicalize(".")?,
        };
        match self.workspace_initialised_from_path(&base_path) { // need to check workspace in base_path
            true => {
                println!("Workspace already initialised");
                std::process::exit(1)
            }
            false => {
                let workspace_dir = ".synth";
                let result = std::fs::create_dir_all(base_path.join(workspace_dir)).context(format!(
                    "Failed to create working directory at: {} during initialization",
                    base_path.join(workspace_dir).to_str().unwrap()
                ));
                let config_path = ".synth/config.toml";
                match result {
                    Ok(()) => {
                        File::create(base_path.join(config_path)).context(format!(
                            "Failed to create config file at: {} during initialization",
                            base_path.join(config_path).to_str().unwrap()
                        ))?;
                        Ok(())
                    }
                    Err(ref e)
                        if e.downcast_ref::<std::io::Error>().unwrap().kind()
                            == std::io::ErrorKind::AlreadyExists =>
                    {
                        File::create(base_path.join(config_path)).context(format!(
                            "Failed to initialize workspace at: {}. File already exists.",
                            base_path.join(config_path).to_str().unwrap()
                        ))?;
                        Ok(())
                    }
                    _ => result,
                }
            }
        }
    }

    fn workspace_initialised(&self) -> bool {
        Path::new(".synth/config.toml").exists()
    }

    fn workspace_initialised_from_path(&self, init_path: &PathBuf) -> bool {
        let config_path = init_path.join(".synth/config.toml");
        Path::new(&config_path).exists()
    }

    fn import(
        &self,
        path: PathBuf,
        collection: Option<Name>,
        import_strategy: Option<SomeImportStrategy>,
    ) -> Result<()> {
        if !self.workspace_initialised() {
            return Err(anyhow!(
                "Workspace has not been initialised. To initialise the workspace run `synth init [optional path]`."
            ));
        }

        if !path.is_relative() {
            return Err(anyhow!(
		"The namespace path `{}` is absolute. Only paths relative to an initialised workspace root are accepted.",
		path.display()
	    ));
        }

        // TODO: If ns exists and no collection: break
        // If collection and ns exists and collection exists: break
        if let Some(collection) = collection {
            if self.store.collection_exists(&path, &collection) {
                return Err(anyhow!(
                    "The collection `{}` already exists. Will not import into an existing collection.",
		    Store::relative_collection_path(&path, &collection).display()
		));
            } else {
                let content = import_strategy
                    .unwrap_or_default()
                    .import_collection(&collection)?;
                self.store
                    .save_collection_path(&path, collection, content)?;
                Ok(())
            }
        } else if self.store.ns_exists(&path) {
            Err(anyhow!(
                "The namespace at `{}` already exists. Will not import into an existing namespace.",
                path.display()
            ))
        } else {
            let ns = import_strategy.unwrap_or_default().import()?;
            self.store.save_ns_path(path, ns)?;
            Ok(())
        }
    }

    fn generate(
        &self,
        ns_path: PathBuf,
        collection: Option<Name>,
        target: usize,
        to: Option<SomeExportStrategy>,
        seed: u64,
    ) -> Result<()> {
        if !self.workspace_initialised() {
            return Err(anyhow!(
                "Workspace has not been initialised. To initialise the workspace run `synth init [optional path]`."
            ));
        }
        let namespace = self
            .store
            .get_ns(ns_path.clone())
            .context("Unable to open the namespace")?;

        let params = ExportParams {
            namespace,
            collection_name: collection,
            target,
            seed,
        };

        to.unwrap_or_default()
            .export(params)
            .context(format!("At namespace {:?}", ns_path))
    }
}

#[derive(StructOpt)]
#[structopt(name = "synth", about = "synthetic data engine on the command line")]
pub enum CliArgs {
    #[structopt(about = "Initialise the workspace")]
    Init {
        #[structopt(parse(from_os_str), help = "name of directory to initialize")]
        init_path: Option<PathBuf>,
    },
    #[structopt(about = "Generate data from a namespace")]
    Generate {
        #[structopt(
            help = "the namespace directory from which to generate",
            parse(from_os_str)
        )]
        namespace: PathBuf,
        #[structopt(long, help = "the specific collection from which to generate")]
        collection: Option<Name>,
        #[structopt(long, help = "the number of samples", default_value = "1")]
        size: usize,
        #[structopt(
            long,
            help = "The sink into which to generate data. Can be a postgres uri, a mongodb uri. If not specified, data will be written to stdout"
        )]
        to: Option<SomeExportStrategy>,
        #[structopt(
            long,
            help = "an unsigned 64 bit integer seed to be used as a seed for generation"
        )]
        seed: Option<u64>,
        #[structopt(
            long,
            help = "generation will use a random seed - this cannot be used with --seed"
        )]
        random: bool,
    },
    #[structopt(about = "Import data from an external source")]
    Import {
        #[structopt(
            help = "The namespace directory into which to import",
            parse(from_os_str)
        )]
        namespace: PathBuf,
        #[structopt(
            long,
            help = "The name of a collection into which the data will be imported"
        )]
        collection: Option<Name>,
        #[structopt(
            long,
            help = "The source from which to import data. Can be a postgres uri, a mongodb uri or a path to a JSON file / directory. If not specified, data will be read from stdin"
        )]
        from: Option<SomeImportStrategy>,
    },
    #[structopt(about = "Toggle anonymous usage data collection")]
    Telemetry(TelemetryCommand),
}

#[derive(StructOpt)]
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
