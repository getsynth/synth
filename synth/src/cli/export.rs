use crate::cli::postgres::PostgresExportStrategy;
use crate::cli::stdf::StdoutExportStrategy;
use crate::cli::mongo::MongoExportStrategy;
use crate::cli::mysql::MySqlExportStrategy;
use crate::cli::jsonlines::JsonLinesExportStrategy;

use anyhow::{Context, Result};

use std::convert::TryFrom;

use crate::datasource::DataSource;
use crate::sampler::{Sampler, SamplerOutput};
use async_std::task;
use synth_core::{Name, Namespace, Value};
use crate::cli::db_utils::DataSourceParams;

pub trait ExportStrategy {
    fn export(&self, params: ExportParams) -> Result<()>;
}

pub struct ExportParams {
    pub namespace: Namespace,
    /// The name of the single collection to generate from if one is specified (via --collection).
    pub collection_name: Option<Name>,
    /// The number of values to generate (specified via --size).
    pub target: usize,
    pub seed: u64,
}

impl TryFrom<DataSourceParams> for Box<dyn ExportStrategy> {
    type Error = anyhow::Error;

    /// Here we exhaustively try to pattern match strings until we get something
    /// that successfully parses. Starting from a file, could be a url to a database etc.
    /// We assume that these can be unambiguously identified for now.
    /// For example, `postgres://...` is not going to be a file on the FS
    fn try_from(params: DataSourceParams) -> Result<Self, Self::Error> {
        match params.uri {
            None => Ok(Box::new(StdoutExportStrategy {})),
            Some(uri) => {
                let export_strategy: Box<dyn ExportStrategy> = if uri.starts_with("postgres://") || uri.starts_with("postgresql://") {
                    Box::new(PostgresExportStrategy {
                        uri,
                        schema: params.schema,
                    })
                } else if uri.starts_with("mongodb://") {
                    Box::new(MongoExportStrategy {
                        uri
                    })
                } else if uri.starts_with("mysql://") || uri.starts_with("mariadb://") {
                    Box::new(MySqlExportStrategy {
                        uri
                    })
                } else if uri == "jsonl" {
                    Box::new(JsonLinesExportStrategy {})
                } else {
                    return Err(anyhow!(
                            "Data sink not recognized. Was expecting one of 'mongodb' or 'postgres' or 'mysql' or 'mariadb' or 'jsonl'"
                    ));
                };
                Ok(export_strategy)
            }
        }
    }
}

pub(crate) fn create_and_insert_values<T: DataSource>(
    params: ExportParams,
    datasource: &T,
) -> Result<()> {
    let sampler = Sampler::try_from(&params.namespace)?;
    let values =
        sampler.sample_seeded(params.collection_name.clone(), params.target, params.seed)?;

    match values {
        SamplerOutput::Collection(collection) => {
            insert_data(datasource, params.collection_name.unwrap().to_string(), &collection)
        }
        SamplerOutput::Namespace(namespace) => {
            for (name, collection) in namespace {
                insert_data(datasource, name, &collection)?;
            };
            Ok(())
        }
    }
}

fn insert_data<T: DataSource>(
    datasource: &T,
    collection_name: String,
    collection: &[Value],
) -> Result<()> {
    task::block_on(datasource.insert_data(collection_name.clone(), collection))
        .with_context(|| format!("Failed to insert data for collection {}", collection_name))
}
