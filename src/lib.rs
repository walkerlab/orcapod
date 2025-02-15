//! Intuitive compute pipeline orchestration with reproducibility, performance, and scalability in
//! mind.

/// Error handling based on enumeration.
pub mod error;
/// Components of the data model.
pub mod model;
/// Interface into container orchestration engine.
pub mod orchestrator;
/// Data persistence is provided by using a store backend.
pub mod store;
mod util;
