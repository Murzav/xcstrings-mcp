# Contributing to xcstrings-mcp

Thank you for your interest in contributing!

## Development Setup

### Prerequisites

- Rust 1.85+ (edition 2024)
- [cargo-nextest](https://nexte.st/) for running tests

### Build

```sh
cargo build
```

### Run Tests

```sh
# Primary test runner
cargo nextest run

# Doctests (nextest doesn't run these)
cargo test --doc

# Single test
cargo nextest run <test_name>
```

### Pre-commit Check

Run this before submitting a PR:

```sh
cargo fmt --check && cargo clippy -- -D warnings && cargo nextest run
```

### Cargo.lock

`Cargo.lock` is committed to the repository. This is a binary crate, so locking dependencies ensures reproducible builds.

## Architecture

Layered architecture: `server` -> `tools` -> `service` -> `model`, with `io` injected via the `FileStore` trait.

- **server** -- MCP tool routing and handler dispatch
- **tools** -- individual tool implementations
- **service** -- pure logic (parser, extractor, merger, validator, formatter)
- **model** -- serde types for `.xcstrings` format, CLDR plural rules, format specifiers
- **io** -- `FileStore` trait + real filesystem implementation with atomic writes

### Layer Rules

- `service/` never touches the filesystem directly. All I/O goes through the `FileStore` trait.
- Dependencies flow downward: `server` -> `tools` -> `service` -> `model`.

## Critical Rules

- **All logging to stderr.** stdout is the MCP protocol transport. A single `println!` will break the connection. Use `tracing` macros only.
- **`serde_json` with `preserve_order` feature.** Without it, key order randomizes and diffs against Xcode output explode.
- **Xcode colon spacing.** Xcode formats JSON with `" : "` (space-colon-space), not `": "`. The formatter in `service/formatter.rs` handles this. This is the most important correctness concern.
- **No `unwrap()` in non-test code.**

## PR Requirements

- All tests pass (`cargo nextest run` + `cargo test --doc`)
- No clippy warnings (`cargo clippy -- -D warnings`)
- Code is formatted (`cargo fmt --check`)
- New features include tests
- MSRV: 1.85 (Rust edition 2024)
