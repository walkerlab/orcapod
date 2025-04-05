//! Intuitive compute pipeline orchestration with reproducibility, performance, and scalability in
//! mind.
extern crate uniffi as uniffi_external;
uniffi_external::setup_scaffolding!();
/// Pure Rust source.
pub mod core;
/// Exposed `CFFI` client based on `UniFFI`.
pub mod uniffi;
