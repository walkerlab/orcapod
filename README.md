# orcapod

### Rust help

#### Run all integration tests

```bash
cargo test --test '*' --package=orcapod -- --exact --nocapture
```

### Interesting Docs/Refs


- gpu docker
  - https://stackoverflow.com/questions/25185405/using-gpu-from-a-docker-container
- struct field iter
  - https://stackoverflow.com/questions/73257747/deriving-a-hashmap-btree-from-a-strcut-of-values-in-rust
- code coverage
  - https://www.reddit.com/r/rust/comments/y3zzze/rust_project_test_coverage/
- serde
  - https://stackoverflow.com/questions/43554679/how-to-fix-lifetime-error-when-function-returns-a-serde-deserialize-type
  - https://www.reddit.com/r/rust/comments/1bo5dle/we_lost_serdeyaml_whats_the_next_one/
- rust test benchmark
  - https://doc.rust-lang.org/unstable-book/library-features/test.html
- rust LLVM compiler explorer
  - https://www.reddit.com/r/rust/comments/xtiqj8/why_is_this_functional_version_faster_than_my_for/
  - https://rust.godbolt.org/
- rust default
  - derivate:
    - https://stackoverflow.com/questions/19650265/is-there-a-faster-shorter-way-to-initialize-variables-in-a-rust-struct
    - https://stackoverflow.com/questions/68346169/is-there-a-short-way-to-implement-default-for-a-struct-that-has-a-field-that-doe
  - https://github.com/idanarye/rust-smart-default
  - native: https://stackoverflow.com/questions/41510424/most-idiomatic-way-to-create-a-default-struct
  - discussion: https://www.reddit.com/r/rust/comments/x8a7i8/is_there_a_way_to_say_default_for_all_the_other/
- rust field validation
  - https://www.reddit.com/r/rust/comments/paflme/idiomatic_way_to_validate_struct_field_values/
  - https://github.com/Keats/validator
- rust debugging
  - https://rustc-dev-guide.rust-lang.org/debugging-support-in-rustc.html#:~:text=LLDB%20uses%20Clang%20to%20compile,like%20GDB%20has%20convenience%20variables.
