use crate::cli::csv::{CsvFileExportStrategy, CsvStdoutExportStrategy};
use crate::cli::json::{JsonFileExportStrategy, JsonStdoutExportStrategy};
use crate::cli::jsonl::{JsonLinesFileExportStrategy, JsonLinesStdoutExportStrategy};
use crate::cli::mongo::MongoExportStrategy;
use crate::cli::mysql::MySqlExportStrategy;
use crate::cli::postgres::PostgresExportStrategy;

use anyhow::{Context, Result};

use std::cell::RefCell;
use std::io::Write;
use std::path::PathBuf;

use crate::datasource::DataSource;
use crate::sampler::SamplerOutput;
use async_std::task;
use synth_core::{DataSourceParams, Namespace, Value};

use super::map_from_uri_query;

pub(crate) trait ExportStrategy {
    fn export(&self, namespace: Namespace, sample: SamplerOutput) -> Result<()>;
}

pub(crate) struct ExportStrategyBuilder<'a, W> {
    params: DataSourceParams<'a>,
    writer: W,
}

impl<'a> TryFrom<DataSourceParams<'a>> for ExportStrategyBuilder<'a, std::io::Stdout> {
    type Error = anyhow::Error;

    fn try_from(params: DataSourceParams<'a>) -> Result<Self, Self::Error> {
        Ok(Self {
            params,
            writer: std::io::stdout(),
        })
    }
}

impl<'a, W> ExportStrategyBuilder<'a, W> {
    pub fn set_writer<N>(self, writer: N) -> ExportStrategyBuilder<'a, N> {
        ExportStrategyBuilder {
            params: self.params,
            writer,
        }
    }
}

impl<'a, 'w, W> ExportStrategyBuilder<'a, W>
where
    W: Write + 'w,
{
    pub fn build(self) -> Result<Box<dyn ExportStrategy + 'w>> {
        let Self { params, writer } = self;

        // Due to all the schemes used, with the exception of 'mongodb', being non-standard (including 'postgres' and
        // 'mysql' suprisingly) it seems simpler to just match based on the scheme string instead of on enum variants.
        let scheme = params.uri.scheme().as_str().to_lowercase();
        let query = map_from_uri_query(params.uri.query());

        let export_strategy: Box<dyn ExportStrategy> = match scheme.as_str() {
            "postgres" | "postgresql" => Box::new(PostgresExportStrategy {
                uri_string: params.uri.to_string(),
                schema: params.schema,
                concurrency: params.concurrency,
            }),
            "mongodb" => Box::new(MongoExportStrategy {
                uri_string: params.uri.to_string(),
                concurrency: params.concurrency,
            }),
            "mysql" | "mariadb" => Box::new(MySqlExportStrategy {
                uri_string: params.uri.to_string(),
                concurrency: params.concurrency,
            }),
            "json" => {
                if params.uri.path() == "" {
                    Box::new(JsonStdoutExportStrategy {
                        writer: RefCell::new(writer),
                    })
                } else {
                    Box::new(JsonFileExportStrategy {
                        from_file: PathBuf::from(params.uri.path().to_string()),
                    })
                }
            }
            "jsonl" => {
                let collection_field_name = query
                    .get("collection_field_name")
                    .unwrap_or(&"type")
                    .to_string();

                if params.uri.path() == "" {
                    Box::new(JsonLinesStdoutExportStrategy {
                        collection_field_name,
                        writer: RefCell::new(writer),
                    })
                } else {
                    Box::new(JsonLinesFileExportStrategy {
                        from_file: PathBuf::from(params.uri.path().to_string()),
                        collection_field_name,
                    })
                }
            }
            "csv" => {
                if params.uri.path() == "" {
                    Box::new(CsvStdoutExportStrategy)
                } else {
                    Box::new(CsvFileExportStrategy {
                        to_dir: PathBuf::from(params.uri.path().to_string()),
                    })
                }
            }
            _ => {
                return Err(anyhow!(
                    "Export URI scheme not recognised. Was expecting one of 'mongodb', 'postgres', 'mysql', 'mariadb', 'json', 'jsonl' or 'csv'."
                ));
            }
        };
        Ok(export_strategy)
    }
}

pub(crate) fn create_and_insert_values<T: DataSource>(
    sample: SamplerOutput,
    datasource: &T,
) -> Result<()> {
    match sample {
        SamplerOutput::Collection(name, value) => {
            insert_data(datasource, name.as_ref(), value)?;
        }
        SamplerOutput::Namespace(namespace) => {
            for (name, value) in namespace.into_iter() {
                insert_data(datasource, name.as_ref(), value)?;
            }
        }
    };

    Ok(())
}

fn insert_data<T: DataSource>(
    datasource: &T,
    collection_name: &str,
    collection: Value,
) -> Result<()> {
    let to_insert = match collection {
        Value::Array(elems) => elems,
        non_array => vec![non_array],
    };
    task::block_on(datasource.insert_data(collection_name, &to_insert[..]))
        .with_context(|| format!("Failed to insert data for collection {collection_name}"))
}
