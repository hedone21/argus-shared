# argus-shared

Shared IPC protocol types for the **Argus** on-device LLM inference framework.

This crate defines the message types exchanged between the two Argus
processes over Unix Domain Socket / TCP / D-Bus (serde JSON):

- **`ManagerMessage` / `EngineCommand`** — manager → engine. One directive type
  carrying a KV-cache byte budget plus the lifecycle commands. The contract never
  names a KV cache technique: the manager sets the budget, the engine picks how to
  reach it.
- **`EngineMessage`** — engine → manager: a heartbeat (KV cache size, TBT, phase)
  and a per-directive response. A command the engine cannot execute is answered
  `Rejected`, which is how the manager learns the engine's action set — there is no
  separate capability exchange.

It has no dependency on the engine or manager crates, so both can depend on it
without a dependency cycle.

## Related crates

- [`argus-engine`](https://github.com/hedone21/argus-engine) — the inference engine.
- [`argus-manager`](https://github.com/hedone21/argus-manager) — the resource manager service.

## Usage

```toml
[dependencies]
argus-shared = { git = "https://github.com/hedone21/argus-shared.git" }
```

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in this crate by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
