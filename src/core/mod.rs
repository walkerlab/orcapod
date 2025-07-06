pub(crate) mod error;
pub(crate) mod graph;
pub(crate) mod orchestrator;
pub(crate) mod pipeline;
pub(crate) mod store;
pub(crate) mod util;

#[cfg(feature = "default")]
pub(crate) mod crypto;
#[cfg(feature = "default")]
pub(crate) mod model;
#[cfg(feature = "default")]
pub(crate) mod operator;

#[cfg(feature = "test")]
#[expect(missing_docs, reason = "debug")]
pub mod crypto;
#[cfg(feature = "test")]
#[expect(
    missing_docs,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    reason = "debug"
)]
pub mod model;
#[cfg(feature = "test")]
#[expect(missing_docs, clippy::missing_errors_doc, reason = "debug")]
pub mod operator;
