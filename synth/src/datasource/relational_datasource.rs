use crate::datasource::DataSource;
use anyhow::Result;
use async_lock::Semaphore;
use async_std::task;
use async_trait::async_trait;
use beau_collector::BeauCollector;
use futures::future::join_all;
use sqlx::{
    query::Query, Arguments, Connection, Database, Encode, Executor, IntoArguments, Pool, Type,
};
use std::sync::Arc;
use synth_core::{Content, Value};
use synth_gen::value::Number;

const DEFAULT_INSERT_BATCH_SIZE: usize = 1000;

//TODO: Remove this once https://github.com/rust-lang/rust/issues/88900 gets fixed
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub(crate) column_name: String,
    pub(crate) ordinal_position: i32,
    pub(crate) is_nullable: bool,
    pub(crate) is_custom_type: bool,
    pub(crate) data_type: String,
    pub(crate) character_maximum_length: Option<i32>,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct PrimaryKey {
    pub(crate) column_name: String,
    pub(crate) type_name: String,
}

#[derive(Debug)]
pub struct ForeignKey {
    pub(crate) from_table: String,
    pub(crate) from_column: String,
    pub(crate) to_table: String,
    pub(crate) to_column: String,
}

/// Wrapper around `Value` since we can't impl `TryFrom` on a struct in a non-owned crate
#[derive(Debug)]
pub struct ValueWrapper(pub(crate) Value);

/// All sqlx databases should define this trait and implement database specific queries in
/// their own implementations.
#[async_trait]
pub trait SqlxDataSource: DataSource {
    type DB: Database<Arguments = Self::Arguments, Connection = Self::Connection>;
    type Arguments: for<'q> Arguments<'q, Database = Self::DB> + for<'q> IntoArguments<'q, Self::DB>;
    type Connection: Connection<Database = Self::DB>;

    const IDENTIFIER_QUOTE: char;

    fn clone(&self) -> Self;

    /// Gets a pool to execute queries with
    fn get_pool(&self) -> Pool<Self::DB>;

    /// Gets a multithread pool to execute queries with
    fn get_multithread_pool(&self) -> Pool<Self::DB>;

    /// Get the maximum concurrency for the data source
    fn get_concurrency(&self) -> usize;

    /// Prepare a single query with data source specifics
    fn query<'q>(&self, query: &'q str) -> Query<'q, Self::DB, Self::Arguments> {
        sqlx::query(query)
    }

    /// Get query for table names
    fn get_table_names_query(&self) -> &str;

    /// Get query for primary keys
    fn get_primary_keys_query(&self) -> &str;

    /// Get query for foreign keys
    fn get_foreign_keys_query(&self) -> &str;

    /// Get query for columns info
    fn get_columns_info_query(&self) -> &str;

    /// Get properly formatted table name
    fn get_table_name_for_insert(&self, table_name: &str) -> String;

    async fn set_seed(&self) -> Result<()> {
        // Default for sources that don't need to set a seed
        Ok(())
    }

    /// Get query for deterministic values
    fn get_deterministic_samples_query(&self, table_name: String) -> String;

    /// Decodes column to our Content
    fn decode_to_content(&self, column_info: &ColumnInfo) -> Result<Content>;

    /// Get the function arguments for datasource
    fn get_function_argument_placeholder(_current: usize, _index: usize, _value: &Value) -> String {
        "?".to_string()
    }

    // Returns extended query string + current index
    fn extend_parameterised_query(query: &mut String, curr_index: usize, query_params: Vec<Value>) {
        let extend = query_params.len();

        query.push('(');
        for (i, param) in query_params.iter().enumerate() {
            query.push_str(&Self::get_function_argument_placeholder(
                curr_index, i, param,
            ));
            if i != extend - 1 {
                query.push(',');
            }
        }
        query.push(')');
    }

    async fn execute_query(
        &self,
        query: String,
        query_params: Vec<Value>,
    ) -> Result<<Self::DB as Database>::QueryResult>
    where
        for<'c> &'c mut Self::Connection: Executor<'c, Database = Self::DB>,
        Value: Type<Self::DB> + Send + Sync,
        for<'d> Value: Encode<'d, Self::DB> + Send + Sync,
    {
        let mut query = sqlx::query::<Self::DB>(query.as_str());

        for param in query_params {
            query = query.bind(param);
        }

        let result = query.execute(&self.get_multithread_pool()).await?;

        Ok(result)
    }
}

pub async fn insert_relational_data<T: SqlxDataSource + Sync + Send + 'static>(
    datasource: &T,
    collection_name: &str,
    collection: &[Value],
) -> Result<()>
where
    for<'c> &'c mut T::Connection: Executor<'c, Database = T::DB>,
    String: Type<T::DB>,
    for<'d> String: Encode<'d, T::DB>,
    ColumnInfo: TryFrom<<T::DB as Database>::Row, Error = anyhow::Error>,
    Value: Type<T::DB>,
    for<'d> Value: Encode<'d, T::DB>,
{
    let batch_size = DEFAULT_INSERT_BATCH_SIZE;
    let max_concurrency = datasource.get_concurrency();

    if collection.is_empty() {
        println!("Collection {collection_name} generated 0 values. Skipping insertion...",);
        return Ok(());
    }

    let column_infos = get_columns_info(datasource, collection_name.to_string()).await?;
    let first_valueset = collection[0]
        .as_object()
        .expect("This is always an object (sampler contract)");

    for column_info in column_infos {
        if let Some(value) = first_valueset.get(&column_info.column_name) {
            match (value, &*column_info.data_type) {
                (
                    Value::Number(Number::U64(_)),
                    "int2" | "int4" | "int8" | "smallint" | "int" | "bigint",
                ) => warn!(
                    "Trying to put an unsigned u64 into a {} typed column {}.{}",
                    column_info.data_type, collection_name, column_info.column_name
                ),
                (
                    Value::Number(Number::U32(_)),
                    "int2" | "int4" | "int8" | "smallint" | "int" | "bigint",
                ) => warn!(
                    "Trying to put an unsigned u32 into a {} typed column {}.{}",
                    column_info.data_type, collection_name, column_info.column_name
                ),
                (Value::Number(Number::I64(_)), "int2" | "int4" | "smallint" | "int") => warn!(
                    "Trying to put a signed i64 into a {} typed column {}.{}",
                    column_info.data_type, collection_name, column_info.column_name
                ),
                (Value::Number(Number::I32(_)), "int2" | "int8" | "smallint" | "bigint") => {
                    warn!(
                        "Trying to put a signed i32 into a {} typed column {}.{}",
                        column_info.data_type, collection_name, column_info.column_name
                    )
                }
                (Value::Number(Number::F64(_)), "float4" | "float") => {
                    warn!(
                        "Trying to put a f64 into a {} typed column {}.{}",
                        column_info.data_type, collection_name, column_info.column_name
                    )
                }
                (Value::Number(Number::F32(_)), "float8" | "double") => {
                    warn!(
                        "Trying to put a f32 into a {} typed column {}.{}",
                        column_info.data_type, collection_name, column_info.column_name
                    )
                }
                //TODO: More variants
                _ => {}
            }
        }
    }

    let column_names = first_valueset
        .keys()
        .map(|k| format!("{}{}{}", T::IDENTIFIER_QUOTE, k, T::IDENTIFIER_QUOTE))
        .collect::<Vec<String>>()
        .join(",");

    let collection_chunks = collection.chunks(batch_size);
    let mut futures = Vec::with_capacity(collection_chunks.len());

    info!(
        "Inserting {} rows for {}...",
        collection.len(),
        collection_name
    );

    let semaphore = Arc::new(Semaphore::new(max_concurrency));
    for rows in collection_chunks {
        let permit = semaphore.clone().acquire_arc().await;

        let table_name = datasource.get_table_name_for_insert(collection_name);
        let mut query = format!("INSERT INTO {table_name} ({column_names}) VALUES \n");

        let mut curr_index = 0;
        let mut query_params = vec![];

        for (i, row) in rows.iter().enumerate() {
            let row_obj = row
                .as_object()
                .expect("This is always an object (sampler contract)");

            let mut curr_query_params: Vec<Value> = row_obj.values().cloned().collect();
            T::extend_parameterised_query(&mut query, curr_index, curr_query_params.clone());
            curr_index += curr_query_params.len();
            query_params.append(&mut curr_query_params);

            if i == rows.len() - 1 {
                query.push_str(";\n");
            } else {
                query.push_str(",\n");
            }
        }
        let datasource = datasource.clone();
        let future = task::spawn(async move {
            let result = datasource.execute_query(query, query_params).await;
            drop(permit);
            result
        });

        futures.push(future);
    }

    let results = join_all(futures).await;

    if let Err(e) = results.into_iter().bcollect::<Vec<_>>() {
        bail!("One or more database inserts failed: {:?}", e)
    }

    info!("Inserted {} rows...", collection.len());
    Ok(())
}

pub async fn get_columns_info<T: SqlxDataSource>(
    datasource: &T,
    table_name: String,
) -> Result<Vec<ColumnInfo>>
where
    for<'c> &'c mut T::Connection: Executor<'c, Database = T::DB>,
    String: Type<T::DB>,
    for<'d> String: Encode<'d, T::DB>,
    ColumnInfo: TryFrom<<T::DB as Database>::Row, Error = anyhow::Error>,
{
    let query = datasource.get_columns_info_query();
    let pool = datasource.get_pool();

    datasource
        .query(query)
        .bind(table_name)
        .fetch_all(&pool)
        .await?
        .into_iter()
        .map(ColumnInfo::try_from)
        .collect()
}
