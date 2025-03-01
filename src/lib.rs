//! Intuitive compute pipeline orchestration with reproducibility, performance, and scalability in
//! mind.

/// State change verification via cryptographic utilities.
pub mod crypto;
/// Error handling based on enumeration.
pub mod error;
/// Components of the data model.
pub mod model;
/// Interface into container orchestration engine.
pub mod orchestrator;
/// Data persistence provided by a store backend.
pub mod store;
mod util;
