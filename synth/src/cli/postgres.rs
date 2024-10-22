use crate::cli::export::{create_and_insert_values, ExportStrategy};
use crate::cli::import::ImportStrategy;
use crate::cli::import_utils::build_namespace_import;
use crate::datasource::postgres_datasource::{PostgresConnectParams, PostgresDataSource};
use crate::datasource::DataSource;
use crate::sampler::SamplerOutput;
use anyhow::Result;
use synth_core::schema::Namespace;

#[derive(Clone, Debug)]
pub struct PostgresExportStrategy {
    pub uri_string: String,
    pub schema: Option<String>,
    pub concurrency: usize,
}

impl ExportStrategy for PostgresExportStrategy {
    fn export(&self, _namespace: Namespace, sample: SamplerOutput) -> Result<()> {
        let connect_params = PostgresConnectParams {
            uri: self.uri_string.clone(),
            schema: self.schema.clone(),
            concurrency: self.concurrency,
        };

        let datasource = PostgresDataSource::new(&connect_params)?;

        create_and_insert_values(sample, &datasource)
    }
}

#[derive(Clone, Debug)]
pub struct PostgresImportStrategy {
    pub uri_string: String,
    pub schema: Option<String>,
}

impl ImportStrategy for PostgresImportStrategy {
    fn import(&self) -> Result<Namespace> {
        let connect_params = PostgresConnectParams {
            uri: self.uri_string.clone(),
            schema: self.schema.clone(),
            concurrency: 1,
        };

        let datasource = PostgresDataSource::new(&connect_params)?;

        build_namespace_import(&datasource)
    }
}
