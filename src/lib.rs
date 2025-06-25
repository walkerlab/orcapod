//! Intuitive compute pipeline orchestration with reproducibility, performance, and scalability in
//! mind.
extern crate uniffi as uniffi_external;
uniffi_external::setup_scaffolding!();
/// Pure Rust source.
pub mod core;
/// Exposed CFFI client based on [uniffi](https://crates.io/crates/uniffi).
///
/// `uniffi` brings a lot of convenience in creating a CFFI but we also must meet several
/// requirements. This means there are several design implications to be aware of.
///
/// For instance, anything exposed over the CFFI boundary with `uniffi` (i.e. children of this
/// `mod`) needs to respect several rules (not exhaustive).
/// 1. No use of Rust generics
/// 1. No use of `const` values in traits
/// 1. Primitives must be owned by the client e.g., `String`, `u8`, `bool`, `f32`, etc.
/// 1. `BtreeMap` isn't supported but `HashMap` is.
/// 1. `async` functions must use the custom derive attribute from the
///    [async-trait](https://crates.io/crates/async-trait) crate
/// 1. Custom types that are to be owned by client:
///    1. are passed by value i.e. as a move
///    1. cannot export any methods associated with them i.e. can't be invoked on client side
/// 1. Custom types that are to be owned by Rust:
///    1. are passed by reference using `Arc<T>`
///    1. can have methods exported i.e. can be invoked on client side
///    1. require exporting constructor methods
///    1. require exporting getter methods (e.g. for each field on a `struct`) since the underlying
///       values are owned by Rust
/// 1. No default trait implementations
/// 1. (Rust limitation) No associated functions in traits e.g. class methods in Python
pub mod uniffi;
