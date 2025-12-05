use std::collections::BTreeMap;

use thiserror::Error;
use tokio::task::JoinError;
use tokio::time::error::Elapsed;
use tracing::instrument;

pub mod types;

#[derive(Error, Debug)]
pub enum Error {
    #[error("SerializationError: {0}")]
    SerializationError(#[source] serde_json::Error),

    #[error("Kube Error: {source}")]
    KubeError {
        #[from]
        source: kube::Error,
    },

    #[error("Finalizer Error: {0}")]
    // NB: awkward type because finalizer::Error embeds the reconciler error (which is this)
    // so boxing this error to break cycles
    FinalizerError(#[source] Box<kube::runtime::finalizer::Error<Error>>),

    #[error("IllegalDocument")]
    IllegalDocument,

    #[error("Timeout waiting for LoadBalancer IP")]
    IPTimeout,

    #[error("Returned Ingress list is empty")]
    IngressListEmpty,

    #[error("Returned Ingress list is missing")]
    IngressListMissing,

    #[error("Error joining all futures: {source}")]
    JoinError {
        #[from]
        source: JoinError,
    },

    #[error("Error waiting for condition: {source}")]
    WaitError {
        #[from]
        source: kube_runtime::wait::Error,
    },

    #[error("Timeout waiting for condition: {source}")]
    WaitTimeout {
        #[from]
        source: Elapsed,
    },

    #[error("Node inputs are not defined")]
    MissingNodeInputs(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    pub fn metric_label(&self) -> String {
        format!("{self:?}").to_lowercase()
    }
}

#[instrument]
pub fn labels(name: String, kind: String) -> BTreeMap<String, String> {
    let mut labels = selector_labels(name, kind);
    labels.insert("app.kubernetes.io/version".to_owned(), "0.1.0".to_owned());
    labels.insert(
        "app.kubernetes.io/managed-by".to_owned(),
        "ipfs-operator".to_owned(),
    );
    labels
}

#[instrument]
pub fn selector_labels(name: String, kind: String) -> BTreeMap<String, String> {
    let mut labels: BTreeMap<String, String> = BTreeMap::new();
    labels.insert(
        "app.kubernetes.io/name".to_owned(),
        format!("ipfs-{kind}-cluster"),
    );
    labels.insert("app.kubernetes.io/instance".to_owned(), name.to_owned());
    labels
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionType {
    Create,
    Update,
}

#[instrument]
pub fn external_address_name(name: &str) -> String {
    format!("{name}-external-addresses")
}
