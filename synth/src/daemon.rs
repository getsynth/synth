use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use std::collections::HashMap;
use std::path::Path;

use chrono::NaiveDateTime;

use anyhow::{Context, Result};

use synth_core::{
    error::{Error, ErrorKind},
    schema::{
        optionalise::{Optionalise, OptionaliseApi},
        s_override::{DefaultOverrideStrategy, OverrideStrategy},
        Content, FieldRef, Name, Namespace, OptionalMergeStrategy,
    },
};

use crate::index::Index;
use crate::sampler::Sampler;
use std::convert::TryFrom;

pub type Document = Value;

#[derive(Serialize, Deserialize)]
pub struct PutDocumentsRequest {
    pub namespace: Name,
    pub collection: Name,
    pub body: PutDocumentsRequestBody,
}

#[derive(Serialize, Deserialize)]
pub struct PutDocumentsRequestBody {
    hint: Option<Value>,
    #[serde(flatten)]
    content: PutDocumentsRequestContent,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PutDocumentsRequestContent {
    Batch(Vec<Document>),
    Document(Document),
}

impl IntoIterator for PutDocumentsRequestContent {
    type IntoIter = std::vec::IntoIter<Document>;
    type Item = Document;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        match self {
            Self::Batch(batch) => batch.into_iter(),
            Self::Document(document) => vec![document].into_iter(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct PutDocumentsResponse;

#[derive(Serialize, Deserialize)]
pub struct PutOverrideRequest {
    pub namespace: Name,
    pub query: PutOverrideRequestQuery,
    pub body: PutOverrideRequestBody,
}

#[derive(Serialize, Deserialize)]
pub struct PutOverrideRequestQuery {
    #[serde(default)]
    pub depth: Option<usize>,
}

#[derive(Serialize, Deserialize)]
pub struct PutOverrideRequestBody {
    pub at: FieldRef,
    #[serde(rename = "override")]
    pub override_: Value,
}

#[derive(Serialize, Deserialize)]
pub struct PutOverrideResponse {}

#[derive(Serialize, Deserialize)]
pub struct DeleteOverrideRequest {
    pub namespace: Name,
    pub body: DeleteOverrideRequestBody,
}

#[derive(Serialize, Deserialize)]
pub struct DeleteOverrideRequestBody {
    pub at: FieldRef,
}

#[derive(Serialize, Deserialize)]
pub struct DeleteOverrideResponse {}

pub struct GetDocumentsSampleRequest {
    pub namespace: Name,
    pub collection: Option<Name>,
    pub body: GetDocumentsSampleRequestBody,
}

#[derive(Serialize, Deserialize)]
pub struct GetDocumentsSampleRequestBody {
    /// @brokad: Ignored
    size: usize,
}

impl Default for GetDocumentsSampleRequestBody {
    fn default() -> Self {
        Self { size: 1 }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum GetDocumentsSampleResponse {
    Namespaced(Map<String, Value>),
    Collection(Vec<Document>),
}

pub struct Daemon {
    index: Index,
}

pub struct GetSchemaRequest {
    pub namespace: Name,
    pub query: GetSchemaRequestQuery,
}

#[derive(Serialize, Deserialize)]
pub struct GetSchemaRequestQuery {
    pub at: Option<FieldRef>,
    pub generation: Option<i32>,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum NamespaceOrContent {
    Namespace(Namespace),
    Content(Content),
}

#[derive(Serialize, Deserialize)]
pub struct PutOptionaliseRequest {
    pub namespace: Name,
    pub body: PutOptionaliseRequestBody,
}

#[derive(Serialize, Deserialize)]
pub struct PutOptionaliseRequestBody {
    #[serde(flatten)]
    pub content: Optionalise,
}

#[derive(Serialize, Deserialize)]
pub struct PutOptionaliseResponse {}

#[derive(Serialize, Deserialize)]
pub struct GetSchemaResponse(NamespaceOrContent);

#[derive(Serialize, Deserialize)]
pub struct DeleteCollectionRequest {
    pub namespace: Name,
    pub collection: Name,
}

#[derive(Serialize, Deserialize)]
pub struct DeleteCollectionResponse;

#[derive(Serialize, Deserialize)]
pub struct RollbackNamespaceRequest {
    pub namespace: Name,
    pub body: RollbackNamespaceRequestBody,
}

#[derive(Serialize, Deserialize, Default)]
pub struct RollbackNamespaceRequestBody {
    generation: i32,
}

#[derive(Serialize, Deserialize)]
pub struct RollbackNamespaceResponse;

#[derive(Serialize, Deserialize)]
pub struct DeleteNamespaceRequest {
    pub namespace: Name,
    pub body: DeleteNamespaceRequestBody,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DeleteNamespaceRequestBody {
    erase: bool,
}

#[derive(Serialize, Deserialize)]
pub struct DeleteNamespaceResponse;

#[derive(Serialize, Deserialize)]
pub struct GetNamespacesRequest;

#[derive(Serialize, Deserialize)]
pub struct GetNamespacesResponse(HashMap<String, GetNamespacesResponseEntry>);

#[derive(Serialize, Deserialize)]
pub struct GetNamespacesResponseEntry {
    current_generation: i32,
    last_updated_at: NaiveDateTime,
}

impl Daemon {
    pub fn new<R: AsRef<Path>>(data_directory: R) -> Result<Self> {
        Ok(Self {
            index: Index::at(data_directory)?,
        })
    }

    fn sample(
        &self,
        namespace: &Namespace,
        collection: Option<Name>,
        target: usize,
    ) -> Result<Value> {
        let value_generator = Sampler::try_from(namespace)?;
        value_generator.sample(collection, target)
    }

    /// Test runs the model generated from the `Namespace` E2E.
    pub fn validate(&self, namespace: &Namespace) -> Result<()> {
        self.sample(namespace, None, 100)?;
        Ok(())
    }

    pub fn get_schema(&self, req: GetSchemaRequest) -> Result<GetSchemaResponse> {
        let namespace = self
            .index
            .borrow_at_gen(&req.namespace, req.query.generation)?;

        if let Some(field_ref) = req.query.at {
            let content = namespace.as_ref().get_s_node(&field_ref)?.clone();
            Ok(GetSchemaResponse(NamespaceOrContent::Content(content)))
        } else {
            Ok(GetSchemaResponse(NamespaceOrContent::Namespace(
                namespace.clone(),
            )))
        }
    }

    pub fn get_namespaces(&self, _req: GetNamespacesRequest) -> Result<GetNamespacesResponse> {
        self.index.list_ns().map(|res| {
            GetNamespacesResponse(
                res.into_iter()
                    .map(|entry| {
                        (
                            entry.namespace,
                            GetNamespacesResponseEntry {
                                current_generation: entry.generation,
                                last_updated_at: entry.timestamp,
                            },
                        )
                    })
                    .collect(),
            )
        })
    }

    pub fn put_optionalise(&self, req: PutOptionaliseRequest) -> Result<PutOptionaliseResponse> {
        let mut namespace = self.index.borrow_mut(&req.namespace)?;

        let optionalise = req.body.content;

        namespace.optionalise(optionalise)?;
        namespace.commit()?;

        Ok(PutOptionaliseResponse {})
    }

    pub fn put_override(&self, req: PutOverrideRequest) -> Result<PutOverrideResponse> {
        let mut namespace = self.index.borrow_mut(&req.namespace)?;
        let strategy = DefaultOverrideStrategy {
            at: &req.body.at,
            depth: req.query.depth,
        };
        strategy.merge(&mut namespace, &req.body.override_)?;

        self.validate(&namespace)
            .with_context(|| anyhow!("while validating the overridden model"))?;

        namespace.commit()?;
        Ok(PutOverrideResponse {})
    }

    pub fn delete_override(&self, req: DeleteOverrideRequest) -> Result<DeleteOverrideResponse> {
        let mut namespace = self.index.borrow_mut(&req.namespace)?;
        let strategy = DefaultOverrideStrategy {
            at: &req.body.at,
            depth: None,
        };
        strategy.delete_from(&mut namespace)?;
        namespace.commit()?;
        Ok(DeleteOverrideResponse {})
    }

    pub fn put_documents(&self, req: PutDocumentsRequest) -> Result<PutDocumentsResponse> {
        let mut namespace = self.index.borrow_mut(&req.namespace).or_else(|err| {
            match err.downcast_ref::<Error>() {
                Some(err) if *err.kind() == ErrorKind::NotFound => {
                    self.index.create_ns(&req.namespace)?;
                    self.index.borrow_mut(&req.namespace)
                }
                _ => Err(err),
            }
        })?;

        let collection = req.collection;

        let documents: Vec<Value> = req.body.content.into_iter().collect();

        if let Some(document) = documents.first() {
            if !namespace.collection_exists(&collection) {
                namespace
                    .create_collection(&collection, document)
                    .with_context(|| anyhow!(
                        "while creating a collection from the first document"
                    ))?;
            }

            if let Some(hint) = req.body.hint {
                let strategy = DefaultOverrideStrategy {
                    at: &collection.clone().into(),
                    depth: None,
                };
                strategy.merge(&mut namespace, &hint)?;
            }

            let as_value = Value::from(documents);
            namespace.try_update(OptionalMergeStrategy, &collection, &as_value)?;
            self.validate(namespace.as_ref()).with_context(|| anyhow!(
                "while validating the inferred model prior to persisting it"
            ))?;

            namespace.commit()?;
        }

        Ok(PutDocumentsResponse)
    }
    pub fn sample_documents(
        &self,
        req: GetDocumentsSampleRequest,
    ) -> Result<GetDocumentsSampleResponse> {
        let namespace = self.index.borrow(&req.namespace)?;

        match self.sample(namespace.as_ref(), req.collection, req.body.size)? {
            Value::Array(arr) => Ok(GetDocumentsSampleResponse::Collection(arr)),
            Value::Object(map) => Ok(GetDocumentsSampleResponse::Namespaced(map)),
            _ => unreachable!(),
        }
    }

    pub fn delete_collection(
        &self,
        req: DeleteCollectionRequest,
    ) -> Result<DeleteCollectionResponse> {
        let mut namespace = self.index.borrow_mut(&req.namespace)?;
        namespace.delete_collection(&req.collection)?;
        namespace.commit()?;
        Ok(DeleteCollectionResponse)
    }

    pub fn delete_namespace(&self, req: DeleteNamespaceRequest) -> Result<DeleteNamespaceResponse> {
        if !req.body.erase {
            return Err(failed!(target: Release,
                "will not delete a namespace if the query parameter 'erase' is not explicitly set to 'true' (this operation cannot be reverted!!)"
            ));
        }

        {
            let namespace = self.index.borrow(&req.namespace)?;
            if !namespace.is_empty() {
                return Err(failed!(
                    target: Release,
                    "will not delete a namespace that has collections: delete {} first",
                    namespace
                        .keys()
                        .map(|k| format!("{},", k))
                        .collect::<String>()
                ));
            }
        }

        self.index.delete_ns(&req.namespace)?;

        Ok(DeleteNamespaceResponse)
    }

    pub fn rollback_namespace(
        &self,
        req: RollbackNamespaceRequest,
    ) -> Result<RollbackNamespaceResponse> {
        self.index
            .rollback_ns(&req.namespace, req.body.generation)?;
        Ok(RollbackNamespaceResponse)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    use tempfile::{tempdir, TempDir};

    struct TempDaemon {
        daemon: Daemon,
        #[allow(dead_code)]
        tempdir: TempDir,
    }

    impl std::ops::Deref for TempDaemon {
        type Target = Daemon;

        fn deref(&self) -> &Self::Target {
            &self.daemon
        }
    }

    fn new_daemon() -> TempDaemon {
        let tempdir = tempdir().unwrap();
        TempDaemon {
            daemon: Daemon::new(&tempdir).unwrap(),
            tempdir,
        }
    }

    #[test]
    fn ingestion_with_hints() {
        let daemon = new_daemon();

        let req = PutDocumentsRequest {
            namespace: "test_ns".parse().unwrap(),
            collection: "test_coll".parse().unwrap(),
            body: PutDocumentsRequestBody {
                hint: Some(serde_json::json!({
                    "content": {
                    "a_date": {
                        "date_time": {
                        "subtype": "naive_date",
                        "format": "%Y-%m-%d",
                        }
                    }
                    }
                })),
                content: PutDocumentsRequestContent::Batch(vec![
                    serde_json::json!({
                    "a_date": "2020-10-2",
                    "a_number": 100
                    }),
                    serde_json::json!({}),
                    serde_json::json!({
                    "a_date": "2020-10-4",
                    "a_number": 200
                    }),
                    serde_json::json!({
                    "a_date": "2020-9-1",
                    "a_number": 50
                    }),
                ]),
            },
        };

        daemon.put_documents(req).unwrap();

        let ns = daemon.index.borrow(&"test_ns".parse().unwrap()).unwrap();

        println!("{}", serde_json::to_string_pretty(ns.as_ref()).unwrap());

        assert_eq!(
            serde_json::json!({
            "test_coll": {
                "type": "array",
                "length": {
                "type": "number",
                "subtype": "u64",
                "range": {
                    "low": 1,
                    "high": 4,
                    "step": 1
                }
                },
                "content": {
                "type": "object",
                "a_number": {
                    "optional": true,
                    "type": "number",
                    "subtype": "u64",
                    "range": {
                    "low": 50,
                    "high": 200,
                    "step": 1
                    }
                },
                "a_date": {
                    "optional": true,
                    "type": "string",
                    "date_time": {
                    "format": "%Y-%m-%d",
                    "subtype": "naive_date",
                    "begin": "2020-09-01",
                    "end": "2020-10-04"
                    }
                }
                }
            }
            }),
            serde_json::to_value(ns.as_ref()).unwrap()
        );
    }
}
