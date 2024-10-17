#!/bin/bash
cargo test --test '*' --package=orcapod -- --exact --nocapture
# cargo llvm-cov --html -- --exact --nocapture
cargo clippy -- -D warnings
# check if rustfmt'ed