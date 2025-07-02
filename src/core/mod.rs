/// State change verification via cryptographic utilities.
pub mod crypto;
pub(crate) mod error;
pub(crate) mod graph;
/// Components of the data model.
pub mod model;
pub(crate) mod orchestrator;
pub(crate) mod store;
pub(crate) mod util;

#[cfg(feature = "default")]
pub(crate) mod operator;

#[cfg(feature = "test")]
#[expect(missing_docs, clippy::missing_errors_doc, reason = "debug")]
pub mod operator;
