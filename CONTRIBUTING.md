# Contributing to argus-shared

Thanks for your interest in contributing!

`argus-shared` defines the IPC protocol types exchanged between the Argus
**manager** and **engine** processes. Changes here affect both
[`argus-engine`](https://github.com/hedone21/argus-engine) and
[`argus-manager`](https://github.com/hedone21/argus-manager) — keep
backward compatibility in mind for serialized message types.

## Development

```bash
cargo build
cargo test
cargo fmt --all
cargo clippy --all-targets -- -D warnings
```

Please make sure `fmt`, `clippy`, and `test` all pass before opening a PR.

## Commit messages

We use [Conventional Commits](https://www.conventionalcommits.org/):
`type(scope): subject` in the imperative mood. Types: `feat`, `fix`, `docs`,
`style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`.

## License of contributions

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you shall be dual licensed under
**MIT OR Apache-2.0**, without any additional terms or conditions.
