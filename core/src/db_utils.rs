use uriparse::URI;

pub struct DataSourceParams<'a> {
    pub uri: URI<'a>,
    pub schema: Option<String>, // PostgreSQL
    pub concurrency: usize,
}
