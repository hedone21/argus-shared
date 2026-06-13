# argus-shared — Agent Guide

Guidance for AI coding agents (Claude Code, Codex, Cursor, ...) working in this
repository. Human contributors should read [CONTRIBUTING.md](CONTRIBUTING.md).

## What this crate is

`argus-shared` is the IPC protocol layer of the Argus framework: the serde
message types exchanged between the **manager** and **engine** processes. It is
a small, dependency-light library (`serde` + `serde_json` only).

## Working agreement

- **Surgical changes.** Touch only what the task requires. Don't reformat or
  refactor unrelated code.
- **Compatibility.** These types are serialized across a process boundary and
  consumed by [`argus-engine`](https://github.com/hedone21/argus-engine) and
  [`argus-manager`](https://github.com/hedone21/argus-manager). Treat changes to
  existing message fields as breaking; prefer additive changes.
- **Verify before done.** Run the checks below and make them pass.

## Build & test

```bash
cargo build
cargo test --all-targets
cargo fmt --all
cargo clippy --all-targets -- -D warnings
```

## Conventions

- **Commits:** Conventional Commits — `type(scope): subject`, imperative mood.
- **License:** contributions are dual licensed `MIT OR Apache-2.0`.
