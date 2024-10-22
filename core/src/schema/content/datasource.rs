use super::prelude::*;
use crate::{DataSourceParams, Value};
use anyhow::Error;
use std::path::PathBuf;
use uriparse::URI;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct DatasourceContent {
    pub path: String,
    #[serde(default)]
    pub cycle: bool,
}

impl Compile for DatasourceContent {
    fn compile<'a, C: Compiler<'a>>(&'a self, mut _compiler: C) -> Result<Graph> {
        let params = DataSourceParams {
            uri: URI::try_from(self.path.as_str())?,
            schema: None,
            concurrency: 1,
        };
        let iter = get_iter(params).map(|i| -> Box<dyn Iterator<Item = Value>> {
            if !self.cycle {
                Box::new(i)
            } else {
                Box::new(i.cycle())
            }
        })?;

        Ok(Graph::Iter(IterNode { iter }))
    }
}

fn get_iter(params: DataSourceParams) -> Result<impl Iterator<Item = Value> + Clone, Error> {
    let scheme = params.uri.scheme().as_str().to_lowercase();

    let iter = match scheme.as_str() {
        "json" => {
            let path = PathBuf::from(params.uri.path().to_string());
            let file = std::fs::File::open(&path).map_err(|e| {
                failed_crate!(
                    target: Release,
                    "failed to open file: {}: {}",
                    path.display(),
                    e
                )
            })?;
            let arr: Vec<Value> = serde_json::from_reader(file).map_err(|e| {
                failed_crate!(
                    target: Release,
                    "failed to read file: {}: {}",
                    path.display(),
                    e
                )
            })?;

            arr.into_iter()
        }
        _ => {
            return Err(anyhow!(
                "Datasource path scheme not recognised. Was expecting 'json'."
            ));
        }
    };

    Ok(iter)
}

#[cfg(test)]
mod tests {
    use super::{Content, DatasourceContent, Generator, GeneratorState};
    use crate::compile::NamespaceCompiler;
    use rand::SeedableRng;
    use std::path::PathBuf;

    #[test]
    fn compile() {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("src/schema/content/test.json");

        let content = DatasourceContent {
            path: format!(
                "json:{}",
                p.into_os_string().into_string().unwrap().replace('\\', "/")
            ),
            cycle: false,
        };

        let content = Content::Datasource(content);
        let compiler = NamespaceCompiler::new_flat(&content);
        compiler.compile().unwrap();
    }

    #[test]
    fn compile_not_cycle() {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("src/schema/content/test.json");

        let content = DatasourceContent {
            path: format!(
                "json:{}",
                p.into_os_string().into_string().unwrap().replace('\\', "/")
            ),
            cycle: false,
        };

        let content = Content::Datasource(content);
        let compiler = NamespaceCompiler::new_flat(&content);
        let mut graph = compiler.compile().unwrap();

        let mut seed = rand::rngs::StdRng::seed_from_u64(5);

        // test.json only has 6 items
        for _ in 0..6 {
            assert!(matches!(
                graph.next(&mut seed),
                GeneratorState::Complete(Ok(_))
            ));
        }
        assert!(matches!(
            graph.next(&mut seed),
            GeneratorState::Complete(Err(_))
        ));
    }

    #[test]
    fn compile_cycle() {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("src/schema/content/test.json");

        let content = DatasourceContent {
            path: format!(
                "json:{}",
                p.into_os_string().into_string().unwrap().replace('\\', "/")
            ),
            cycle: true,
        };

        let content = Content::Datasource(content);
        let compiler = NamespaceCompiler::new_flat(&content);
        let mut graph = compiler.compile().unwrap();

        let mut seed = rand::rngs::StdRng::seed_from_u64(5);

        // test.json only has 6 items, but cycle is true
        for _ in 0..60 {
            assert!(matches!(
                graph.next(&mut seed),
                GeneratorState::Complete(Ok(_))
            ));
        }
    }

    #[test]
    #[should_panic(expected = "scheme not recognised")]
    fn compile_unsupported_scheme() {
        let content = DatasourceContent {
            path: "mysql:".to_string(),
            cycle: false,
        };

        let content = Content::Datasource(content);
        let compiler = NamespaceCompiler::new_flat(&content);
        compiler.compile().unwrap();
    }

    #[test]
    #[should_panic(expected = "failed to open file: missing.json: ")]
    fn compile_file_not_found() {
        let content = DatasourceContent {
            path: "json:missing.json".to_string(),
            cycle: false,
        };

        let content = Content::Datasource(content);
        let compiler = NamespaceCompiler::new_flat(&content);
        compiler.compile().unwrap();
    }

    #[test]
    #[should_panic(expected = "failed to read file: ")]
    fn compile_not_array() {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("src/schema/content/invalid.json");

        let content = DatasourceContent {
            path: format!(
                "json:{}",
                p.into_os_string().into_string().unwrap().replace('\\', "/")
            ),
            cycle: false,
        };

        let content = Content::Datasource(content);
        let compiler = NamespaceCompiler::new_flat(&content);
        compiler.compile().unwrap();
    }
}
