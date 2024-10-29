use crate::datasource::relational_datasource::{
    insert_relational_data, ColumnInfo, ForeignKey, PrimaryKey, SqlxDataSource, ValueWrapper,
};
use crate::datasource::DataSource;
use anyhow::{Context, Result};
use async_std::sync::Arc;
use async_std::task;
use async_trait::async_trait;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use sqlx::postgres::{PgColumn, PgPoolOptions, PgRow, PgTypeInfo, PgTypeKind};
use sqlx::{Column, Executor, Pool, Postgres, Row, TypeInfo};
use std::collections::BTreeMap;
use synth_core::schema::number_content::{F32, F64, I16, I32, I64};
use synth_core::schema::{
    ArrayContent, BoolContent, Categorical, ChronoValue, ChronoValueAndFormat, ChronoValueType,
    DateTimeContent, FakerContent, NumberContent, ObjectContent, RangeStep, RegexContent,
    StringContent, Uuid,
};
use synth_core::{Content, Value};

pub struct PostgresConnectParams {
    pub(crate) uri: String,
    pub(crate) schema: Option<String>,
}

pub struct PostgresDataSource {
    pool: Pool<Postgres>,
    single_thread_pool: Pool<Postgres>,
    schema: String, // consider adding a type schema
}

#[async_trait]
impl DataSource for PostgresDataSource {
    type ConnectParams = PostgresConnectParams;

    fn new(connect_params: &Self::ConnectParams) -> Result<Self> {
        task::block_on(async {
            let schema = connect_params
                .schema
                .clone()
                .unwrap_or_else(|| "public".to_string());

            let mut arc = Arc::new(schema.clone());
            let pool = PgPoolOptions::new()
                .max_connections(3) //TODO expose this as a user config?
                .after_connect(move |conn, _meta| {
                    let schema = arc.clone();
                    Box::pin(async move {
                        conn.execute(&*format!("SET search_path = '{schema}';"))
                            .await?;
                        Ok(())
                    })
                })
                .connect(connect_params.uri.as_str())
                .await?;

            // Needed for queries that require explicit synchronous order, i.e. setseed + random
            arc = Arc::new(schema.clone());
            let single_thread_pool = PgPoolOptions::new()
                .max_connections(1)
                .after_connect(move |conn, _meta| {
                    let schema = arc.clone();
                    Box::pin(async move {
                        conn.execute(&*format!("SET search_path = '{schema}';"))
                            .await?;
                        Ok(())
                    })
                })
                .connect(connect_params.uri.as_str())
                .await?;

            // Better to do the check now and return a helpful error
            Self::check_schema_exists(&single_thread_pool, &schema).await?;

            Ok(PostgresDataSource {
                pool,
                single_thread_pool,
                schema,
            })
        })
    }

    async fn insert_data(&self, collection_name: &str, collection: &[Value]) -> Result<()> {
        insert_relational_data(self, collection_name, collection).await
    }
}

impl PostgresDataSource {
    async fn check_schema_exists(pool: &Pool<Postgres>, schema: &str) -> Result<()> {
        let query = r"SELECT schema_name
        FROM information_schema.schemata
        WHERE catalog_name = current_catalog;";

        let available_schemas: Vec<String> = sqlx::query(query)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|row| row.get(0))
            .collect();

        if !available_schemas.contains(&schema.to_string()) {
            let formatted_schemas = available_schemas.join(", ");
            bail!(
                "the schema '{}' could not be found on the database. Available schemas are: {}.",
                schema,
                formatted_schemas
            );
        }

        Ok(())
    }
}

#[async_trait]
impl SqlxDataSource for PostgresDataSource {
    type DB = Postgres;
    type Arguments = sqlx::postgres::PgArguments;
    type Connection = sqlx::postgres::PgConnection;

    const IDENTIFIER_QUOTE: char = '\"';

    fn get_pool(&self) -> Pool<Self::DB> {
        Pool::clone(&self.single_thread_pool)
    }

    fn get_multithread_pool(&self) -> Pool<Self::DB> {
        Pool::clone(&self.pool)
    }

    fn query<'q>(&self, query: &'q str) -> sqlx::query::Query<'q, Self::DB, Self::Arguments> {
        sqlx::query(query).bind(self.schema.clone())
    }

    fn get_table_names_query(&self) -> &str {
        r"SELECT table_name
        FROM information_schema.tables
        WHERE table_catalog = current_catalog
        AND table_schema = $1
        AND table_type = 'BASE TABLE'"
    }

    fn get_primary_keys_query(&self) -> &str {
        r#"SELECT a.attname, format_type(a.atttypid, a.atttypmod) AS data_type
        FROM pg_index i
        JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
        WHERE  i.indrelid = cast(concat('"', $2, '"') as regclass) AND i.indisprimary"#
    }

    fn get_foreign_keys_query(&self) -> &str {
        r"SELECT tc.table_name, kcu.column_name, ccu.table_name AS foreign_table_name,
            ccu.column_name AS foreign_column_name
            FROM information_schema.table_constraints AS tc
            JOIN information_schema.key_column_usage AS kcu
            ON tc.constraint_name = kcu.constraint_name
            JOIN information_schema.constraint_column_usage AS ccu
            ON ccu.constraint_name = tc.constraint_name
            WHERE constraint_type = 'FOREIGN KEY'
            and tc.table_schema = $1
            and tc.table_catalog = current_catalog"
    }

    /// Must use the singled threaded pool when setting this in conjunction with random, called by
    /// [get_deterministic_samples]. Otherwise, expect endless facepalms (-_Q)
    async fn set_seed(&self) -> Result<()> {
        sqlx::query("SELECT setseed(0.5)")
            .execute(&self.single_thread_pool)
            .await?;
        Ok(())
    }

    fn get_deterministic_samples_query(&self, table_name: String) -> String {
        format!("SELECT * FROM \"{table_name}\" ORDER BY random() LIMIT 10",)
    }

    fn get_table_name_for_insert(&self, table_name: &str) -> String {
        format!("\"{table_name}\"")
    }

    fn decode_to_content(&self, column_info: &ColumnInfo) -> Result<Content> {
        if column_info.is_custom_type {
            return Ok(Content::String(StringContent::Categorical(
                Categorical::default(),
            )));
        }

        let content = match column_info.data_type.to_lowercase().as_str() {
            "bool" => Content::Bool(BoolContent::default()),
            "oid" => {
                bail!("OID data type not supported")
            }
            "char" | "varchar" | "text" | "citext" | "bpchar" | "name" | "unknown" => {
                let pattern = "[a-zA-Z0-9]{0, {}}".replace(
                    "{}",
                    &format!("{}", column_info.character_maximum_length.unwrap_or(1)),
                );
                Content::String(StringContent::Pattern(
                    RegexContent::pattern(pattern).context("pattern will always compile")?,
                ))
            }
            "int2" => Content::Number(NumberContent::I16(I16::Range(RangeStep::default()))),
            "int4" => Content::Number(NumberContent::I32(I32::Range(RangeStep::default()))),
            "int8" => Content::Number(NumberContent::I64(I64::Range(RangeStep::default()))),
            "float4" => Content::Number(NumberContent::F32(F32::Range(RangeStep::default()))),
            "float8" => Content::Number(NumberContent::F64(F64::Range(RangeStep::default()))),
            "numeric" => Content::Number(NumberContent::F64(F64::Range(RangeStep::default()))),
            "timestamptz" => Content::DateTime(DateTimeContent {
                format: "%Y-%m-%dT%H:%M:%S%z".to_string(),
                type_: ChronoValueType::DateTime,
                begin: None,
                end: None,
            }),
            "timestamp" => Content::DateTime(DateTimeContent {
                format: "%Y-%m-%dT%H:%M:%S".to_string(),
                type_: ChronoValueType::NaiveDateTime,
                begin: None,
                end: None,
            }),
            "date" => Content::DateTime(DateTimeContent {
                format: "%Y-%m-%d".to_string(),
                type_: ChronoValueType::NaiveDate,
                begin: None,
                end: None,
            }),
            "time" => Content::DateTime(DateTimeContent {
                format: "%H:%M:%S".to_string(),
                type_: ChronoValueType::NaiveTime,
                begin: None,
                end: None,
            }),
            "json" | "jsonb" => Content::Object(ObjectContent {
                skip_when_null: false,
                fields: BTreeMap::new(),
            }),
            "uuid" => Content::String(StringContent::Uuid(Uuid)),
            "inet" => {
                let arg: synth_core::graph::string::faker::FakerArgs = Default::default();
                Content::String(StringContent::Faker(FakerContent {
                    generator: "ip".to_string(),
                    locales: Vec::new(),
                    args: arg,
                }))
            }
            _ => {
                if let Some(data_type) = column_info.data_type.strip_prefix('_') {
                    let mut column_info = column_info.clone();
                    column_info.data_type = data_type.to_string();

                    Content::Array(ArrayContent::from_content_default_length(
                        self.decode_to_content(&column_info)?,
                    ))
                } else {
                    bail!(
                        "We haven't implemented a converter for {}",
                        column_info.data_type
                    )
                }
            }
        };

        Ok(content)
    }

    fn get_function_argument_placeholder(current: usize, index: usize, value: &Value) -> String {
        let extra = if let Value::Array(_) = value {
            let (typ, depth) = value.get_postgres_type();
            if typ == "unknown" {
                "".to_string() // This is currently not supported
            } else if typ == "jsonb" {
                "::jsonb".to_string() // Cannot have an array of jsonb - ie jsonb[]
            } else {
                format!("::{}{}", typ, "[]".repeat(depth))
            }
        } else {
            "".to_string()
        };

        format!("${}{}", current + index + 1, extra)
    }

    fn get_columns_info_query(&self) -> &str {
        r"SELECT column_name, ordinal_position, is_nullable, udt_name,
        character_maximum_length, data_type
        FROM information_schema.columns
        WHERE table_name = $2
        AND table_schema = $1
        AND table_catalog = current_catalog"
    }
}

impl TryFrom<PgRow> for ColumnInfo {
    type Error = anyhow::Error;

    fn try_from(row: PgRow) -> Result<Self, Self::Error> {
        Ok(ColumnInfo {
            column_name: row.try_get(0)?,
            ordinal_position: row.try_get(1)?,
            is_nullable: row.try_get::<String, usize>(2)? == *"YES",
            data_type: row.try_get(3)?,
            character_maximum_length: row.try_get(4)?,
            is_custom_type: row.try_get::<String, usize>(5)? == "USER-DEFINED",
        })
    }
}

impl TryFrom<PgRow> for PrimaryKey {
    type Error = anyhow::Error;

    fn try_from(row: PgRow) -> Result<Self, Self::Error> {
        Ok(PrimaryKey {
            column_name: row.try_get(0)?,
            type_name: row.try_get(1)?,
        })
    }
}

impl TryFrom<PgRow> for ForeignKey {
    type Error = anyhow::Error;

    fn try_from(row: PgRow) -> Result<Self, Self::Error> {
        Ok(ForeignKey {
            from_table: row.try_get(0)?,
            from_column: row.try_get(1)?,
            to_table: row.try_get(2)?,
            to_column: row.try_get(3)?,
        })
    }
}

impl TryFrom<PgRow> for ValueWrapper {
    type Error = anyhow::Error;

    fn try_from(row: PgRow) -> Result<Self, Self::Error> {
        let mut kv = BTreeMap::new();

        for column in row.columns() {
            let value = try_match_value(&row, column).unwrap_or_else(|err| {
                debug!("try_match_value failed: {}", err);
                Value::Null(())
            });
            kv.insert(column.name().to_string(), value);
        }

        Ok(ValueWrapper(Value::Object(kv)))
    }
}

fn try_match_value(row: &PgRow, column: &PgColumn) -> Result<Value> {
    if let PgTypeKind::Enum(_) = column.type_info().kind() {
        let s = row.try_get::<EnumType, &str>(column.name())?;
        return Ok(Value::String(s.into()));
    }

    let value = match column.type_info().name().to_lowercase().as_str() {
        "bool" => Value::Bool(row.try_get::<bool, &str>(column.name())?),
        "oid" => {
            bail!("OID data type not supported for Postgresql")
        }
        "char" | "varchar" | "text" | "citext" | "bpchar" | "name" | "unknown" => {
            Value::String(row.try_get::<String, &str>(column.name())?)
        }
        "int2" => Value::Number(row.try_get::<i16, &str>(column.name())?.into()),
        "int4" => Value::Number(row.try_get::<i32, &str>(column.name())?.into()),
        "int8" => Value::Number(row.try_get::<i64, &str>(column.name())?.into()),
        "float4" => Value::Number(row.try_get::<f32, &str>(column.name())?.into()),
        "float8" => Value::Number(row.try_get::<f64, &str>(column.name())?.into()),
        "numeric" => {
            let as_decimal = row.try_get::<Decimal, &str>(column.name())?;

            if let Some(truncated) = as_decimal.to_f64() {
                return Ok(Value::Number(truncated.into()));
            }

            bail!("Failed to convert Postgresql numeric data type to 64 bit float")
        }
        "timestampz" => Value::String(row.try_get::<String, &str>(column.name())?),
        "timestamp" => Value::String(row.try_get::<String, &str>(column.name())?),
        "date" => Value::String(format!(
            "{}",
            row.try_get::<chrono::NaiveDate, &str>(column.name())?
        )),
        "time" => Value::String(format!(
            "{}",
            row.try_get::<chrono::NaiveTime, &str>(column.name())?
        )),
        "json" | "jsonb" => {
            let serde_value = row.try_get::<serde_json::Value, &str>(column.name())?;
            serde_json::from_value(serde_value)?
        }
        "char[]" | "varchar[]" | "text[]" | "citext[]" | "bpchar[]" | "name[]" | "unknown[]" => {
            Value::Array(
                row.try_get::<Vec<String>, &str>(column.name())
                    .map(|vec| vec.iter().map(|s| Value::String(s.to_string())).collect())?,
            )
        }
        "bool[]" => Value::Array(
            row.try_get::<Vec<bool>, &str>(column.name())
                .map(|vec| vec.into_iter().map(Value::Bool).collect())?,
        ),
        "int2[]" => Value::Array(
            row.try_get::<Vec<i16>, &str>(column.name())
                .map(|vec| vec.into_iter().map(|i| Value::Number(i.into())).collect())?,
        ),
        "int4[]" => Value::Array(
            row.try_get::<Vec<i32>, &str>(column.name())
                .map(|vec| vec.into_iter().map(|i| Value::Number(i.into())).collect())?,
        ),
        "int8[]" => Value::Array(
            row.try_get::<Vec<i64>, &str>(column.name())
                .map(|vec| vec.into_iter().map(|i| Value::Number(i.into())).collect())?,
        ),
        "float4[]" => Value::Array(
            row.try_get::<Vec<f32>, &str>(column.name())
                .map(|vec| vec.into_iter().map(|i| Value::Number(i.into())).collect())?,
        ),
        "float8[]" => Value::Array(
            row.try_get::<Vec<f64>, &str>(column.name())
                .map(|vec| vec.into_iter().map(|i| Value::Number(i.into())).collect())?,
        ),
        "numeric[]" => {
            let vec = row.try_get::<Vec<Decimal>, &str>(column.name())?;
            let result: Result<Vec<Value>, _> = vec
                .into_iter()
                .map(|d| {
                    if let Some(truncated) = d.to_f64() {
                        return Ok(Value::Number(truncated.into()));
                    }

                    bail!("Failed to convert Postgresql numeric data type to 64 bit float")
                })
                .collect();

            Value::Array(result?)
        }
        "timestamp[]" => Value::Array(
            row.try_get::<Vec<chrono::NaiveDateTime>, &str>(column.name())
                .map(|vec| {
                    vec.into_iter()
                        .map(|d| {
                            Value::DateTime(ChronoValueAndFormat {
                                format: Arc::from("%Y-%m-%dT%H:%M:%S".to_owned()),
                                value: ChronoValue::NaiveDateTime(d),
                            })
                        })
                        .collect()
                })?,
        ),
        "timestamptz[]" => Value::Array(
            row.try_get::<Vec<chrono::DateTime<chrono::FixedOffset>>, &str>(column.name())
                .map(|vec| {
                    vec.into_iter()
                        .map(|d| {
                            Value::DateTime(ChronoValueAndFormat {
                                format: Arc::from("%Y-%m-%dT%H:%M:%S%z".to_owned()),
                                value: ChronoValue::DateTime(d),
                            })
                        })
                        .collect()
                })?,
        ),
        "date[]" => Value::Array(
            row.try_get::<Vec<chrono::NaiveDate>, &str>(column.name())
                .map(|vec| {
                    vec.into_iter()
                        .map(|d| {
                            Value::DateTime(ChronoValueAndFormat {
                                format: Arc::from("%Y-%m-%d".to_owned()),
                                value: ChronoValue::NaiveDate(d),
                            })
                        })
                        .collect()
                })?,
        ),
        "time[]" => Value::Array(
            row.try_get::<Vec<chrono::NaiveTime>, &str>(column.name())
                .map(|vec| {
                    vec.into_iter()
                        .map(|t| {
                            Value::DateTime(ChronoValueAndFormat {
                                format: Arc::from("%H:%M:%S".to_owned()),
                                value: ChronoValue::NaiveTime(t),
                            })
                        })
                        .collect()
                })?,
        ),
        _ => {
            bail!(
                "Could not convert value. Converter not implemented for {}",
                column.type_info().name()
            );
        }
    };

    Ok(value)
}

/// Special type for importing enums
struct EnumType(String);

impl sqlx::types::Type<Postgres> for EnumType {
    fn type_info() -> PgTypeInfo {
        <&str as sqlx::types::Type<Postgres>>::type_info()
    }

    fn compatible(ty: &PgTypeInfo) -> bool {
        matches!(ty.kind(), PgTypeKind::Enum(_))
    }
}

impl<'r> sqlx::Decode<'r, Postgres> for EnumType {
    fn decode(
        value: sqlx::postgres::PgValueRef<'r>,
    ) -> std::result::Result<Self, Box<dyn std::error::Error + 'static + Send + Sync>> {
        Ok(Self(<String as sqlx::Decode<Postgres>>::decode(value)?))
    }
}

impl From<EnumType> for String {
    fn from(enum_type: EnumType) -> Self {
        enum_type.0
    }
}
