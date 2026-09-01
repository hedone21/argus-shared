//! IPC contract between the ARGUS **Manager** and **Engine** processes.
//!
//! Three message types, self-describing JSON, over an abstracted transport:
//!
//! | Direction | Message | Trigger |
//! |---|---|---|
//! | E → M | [`EngineMessage::Heartbeat`] | periodic |
//! | E → M | [`EngineMessage::Response`] | after a directive |
//! | M → E | [`ManagerMessage::Directive`] | when the policy decides |
//!
//! Two properties are load-bearing, and both are why this crate is small:
//!
//! **The contract never names a KV cache technique.** The Manager sets a budget; the
//! Engine picks which of its techniques reaches it. Adding, removing or renaming a
//! technique in the Engine does not touch this crate, which is what lets an existing
//! runtime be integrated by implementing three messages.
//!
//! **There is no capability exchange.** A command the Engine cannot execute is answered
//! [`CommandResult::Rejected`], and that is how the Manager learns what the Engine can
//! do. A separate capability message would be a second thing to keep in sync, and it
//! could not describe a capability that comes and goes with configuration.

use serde::{Deserialize, Deserializer, Serialize};

// ── Engine → Manager ─────────────────────────────────────────

/// Engine operational state reported to the Manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineState {
    Idle,
    Running,
    Suspended,
}

/// Which part of the inference loop the engine is in.
///
/// Prefill and decode load the device very differently — prefill is compute-dense and
/// batched, decode is bandwidth-bound and one token at a time — so a policy that reads
/// only utilization cannot tell a busy engine from a stalled one without this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Idle,
    Prefill,
    Decode,
}

/// Engine status heartbeat: the state only the engine can see.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStatus {
    /// Bytes the resident KV cache currently occupies.
    ///
    /// A real byte count, not a token count scaled by a constant: it follows the cache's
    /// actual dtype, so a quantized cache reports less than an f16 one at equal length.
    /// The distinction matters because [`EngineCommand::KvCompress`] is denominated in
    /// bytes — against a geometry-derived figure the constant would cancel and the budget
    /// would silently be a token ratio.
    pub kv_cache_bytes: u64,
    /// Bytes the same cache would occupy at full capacity, uncompressed. The denominator
    /// of [`EngineCommand::KvCompress::budget`].
    pub kv_cache_budget_bytes: u64,
    /// Resident tokens.
    pub kv_cache_tokens: usize,
    /// Recent time between tokens, in **milliseconds per token** (smoothed).
    /// Note the direction: larger is slower.
    pub tbt_ms: f32,
    pub phase: Phase,
    pub state: EngineState,
}

/// What executing a single command produced.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CommandResult {
    /// The requested state holds.
    Ok,
    /// The command ran but fell short. `achieved` is in the command's own unit — for
    /// [`EngineCommand::KvCompress`] the retained byte fraction actually reached, which
    /// can be `1.0` when the engine declined to act at all.
    Partial { achieved: f32, reason: String },
    /// The engine cannot execute this command in its current configuration. This is the
    /// only signal the Manager gets about the engine's action set, so the reason should
    /// say which of the two it is: unsupported, or unavailable right now.
    Rejected { reason: String },
}

/// Response to an [`EngineDirective`], carrying its `seq_id`.
///
/// `results[i]` answers `commands[i]`: same length, same order, one response per
/// directive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResponse {
    pub seq_id: u64,
    pub results: Vec<CommandResult>,
}

/// Top-level message from Engine to Manager.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EngineMessage {
    Heartbeat(EngineStatus),
    Response(CommandResponse),
}

// ── Manager → Engine ─────────────────────────────────────────

/// Reject a budget that is not a usable fraction, at the point of deserialization.
///
/// Range is checked here rather than in the engine because a non-finite value cannot
/// reach the engine to be rejected: `serde_json` writes NaN and infinity as `null`, and
/// `null` will not deserialize into `f32`, so the whole frame becomes unparseable and is
/// dropped without a response. Failing on the value that *did* arrive at least names the
/// field. Producers must still keep the value finite — a `0 / 0` in a policy script
/// reaches the wire as `null`, and no amount of validation on this side recovers it.
fn de_budget<'de, D: Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
    use serde::de::Error;
    let v = f32::deserialize(d)?;
    if !v.is_finite() {
        return Err(D::Error::custom(format!("budget must be finite, got {v}")));
    }
    if !(v > 0.0 && v <= 1.0) {
        return Err(D::Error::custom(format!(
            "budget must be in (0.0, 1.0], got {v}"
        )));
    }
    Ok(v)
}

/// Manager → Engine command.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EngineCommand {
    /// Shrink the resident KV cache to `budget`.
    ///
    /// `budget` is the fraction of the **uncompressed KV byte** footprint to retain, in
    /// `(0.0, 1.0]`. It is not a token count and not a token ratio; the denominator is
    /// [`EngineStatus::kv_cache_budget_bytes`].
    ///
    /// Which technique meets the budget is the Engine's decision, so this command carries
    /// no technique name and gains no field when the Engine's technique set changes.
    #[serde(rename = "kv.compress")]
    KvCompress {
        #[serde(deserialize_with = "de_budget")]
        budget: f32,
    },

    /// Release everything a previous command applied, returning the engine to its
    /// configured defaults.
    RestoreDefaults,

    /// Suspend inference.
    Suspend,

    /// Resume from suspended.
    Resume,
}

/// Batch of commands from Manager to Engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineDirective {
    pub seq_id: u64,
    pub commands: Vec<EngineCommand>,
}

/// Top-level message from Manager to Engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ManagerMessage {
    Directive(EngineDirective),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip<T: Serialize + for<'de> Deserialize<'de>>(v: &T) -> T {
        serde_json::from_str(&serde_json::to_string(v).unwrap()).unwrap()
    }

    #[test]
    fn kv_compress_wire_tag_is_stable() {
        let json = serde_json::to_string(&EngineCommand::KvCompress { budget: 0.25 }).unwrap();
        assert!(json.contains("\"type\":\"kv.compress\""), "{json}");
        assert!(json.contains("\"budget\":0.25"), "{json}");
        assert_eq!(
            roundtrip(&EngineCommand::KvCompress { budget: 0.25 }),
            EngineCommand::KvCompress { budget: 0.25 }
        );
    }

    #[test]
    fn lifecycle_commands_round_trip() {
        for cmd in [
            EngineCommand::RestoreDefaults,
            EngineCommand::Suspend,
            EngineCommand::Resume,
        ] {
            assert_eq!(roundtrip(&cmd), cmd);
        }
    }

    /// The command set is the contract's whole vocabulary; a variant added without
    /// thought is a technique name leaking back onto the wire.
    #[test]
    fn budget_out_of_range_is_refused() {
        for bad in ["0.0", "-0.5", "1.5", "null"] {
            let json = format!("{{\"type\":\"kv.compress\",\"budget\":{bad}}}");
            assert!(
                serde_json::from_str::<EngineCommand>(&json).is_err(),
                "budget {bad} should not deserialize"
            );
        }
        for good in ["0.25", "1.0", "1e-6"] {
            let json = format!("{{\"type\":\"kv.compress\",\"budget\":{good}}}");
            serde_json::from_str::<EngineCommand>(&json)
                .unwrap_or_else(|e| panic!("budget {good} should deserialize: {e}"));
        }
    }

    /// A non-finite float never survives serialization, so validation on the receiving
    /// side cannot be the only guard — this pins the mechanism so the comment on
    /// [`de_budget`] stays true.
    #[test]
    fn non_finite_budget_serializes_to_null_and_then_fails() {
        let json = serde_json::to_string(&EngineCommand::KvCompress { budget: f32::NAN }).unwrap();
        assert!(json.contains("\"budget\":null"), "{json}");
        assert!(serde_json::from_str::<EngineCommand>(&json).is_err());
    }

    #[test]
    fn directive_carries_seq_id_and_order() {
        let d = EngineDirective {
            seq_id: 7,
            commands: vec![
                EngineCommand::KvCompress { budget: 0.5 },
                EngineCommand::Suspend,
            ],
        };
        let back = match roundtrip(&ManagerMessage::Directive(d)) {
            ManagerMessage::Directive(d) => d,
        };
        assert_eq!(back.seq_id, 7);
        assert_eq!(back.commands.len(), 2);
        assert_eq!(back.commands[0], EngineCommand::KvCompress { budget: 0.5 });
        assert_eq!(back.commands[1], EngineCommand::Suspend);
    }

    /// `PartialEq` on the command vector is what the Manager's directive deduplicator
    /// compares, so the derive is load-bearing rather than incidental.
    #[test]
    fn commands_compare_by_value() {
        let a = vec![EngineCommand::KvCompress { budget: 0.5 }];
        assert_eq!(a, vec![EngineCommand::KvCompress { budget: 0.5 }]);
        assert_ne!(a, vec![EngineCommand::KvCompress { budget: 0.4 }]);
    }

    #[test]
    fn engine_messages_round_trip() {
        let hb = EngineMessage::Heartbeat(EngineStatus {
            kv_cache_bytes: 1024,
            kv_cache_budget_bytes: 4096,
            kv_cache_tokens: 32,
            tbt_ms: 12.5,
            phase: Phase::Decode,
            state: EngineState::Running,
        });
        let json = serde_json::to_string(&hb).unwrap();
        assert!(json.contains("\"type\":\"heartbeat\""), "{json}");
        assert!(json.contains("\"phase\":\"decode\""), "{json}");
        match roundtrip(&hb) {
            EngineMessage::Heartbeat(s) => {
                assert_eq!(s.kv_cache_bytes, 1024);
                assert_eq!(s.kv_cache_budget_bytes, 4096);
                assert_eq!(s.phase, Phase::Decode);
                assert_eq!(s.state, EngineState::Running);
            }
            other => panic!("expected heartbeat, got {other:?}"),
        }
    }

    /// One response per directive, `results[i]` answering `commands[i]`.
    #[test]
    fn response_results_round_trip_per_status() {
        let resp = EngineMessage::Response(CommandResponse {
            seq_id: 3,
            results: vec![
                CommandResult::Ok,
                CommandResult::Partial {
                    achieved: 1.0,
                    reason: "eviction declined".to_string(),
                },
                CommandResult::Rejected {
                    reason: "not configured".to_string(),
                },
            ],
        });
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"status\":\"ok\""), "{json}");
        assert!(json.contains("\"status\":\"partial\""), "{json}");
        assert!(json.contains("\"status\":\"rejected\""), "{json}");
        match roundtrip(&resp) {
            EngineMessage::Response(r) => {
                assert_eq!(r.seq_id, 3);
                assert_eq!(r.results.len(), 3);
                assert_eq!(r.results[0], CommandResult::Ok);
            }
            other => panic!("expected response, got {other:?}"),
        }
    }

    /// A heartbeat missing one of the six fields is an error, not a zero. Every field is
    /// something only the engine can observe, so a default would be a fabrication — and
    /// a version-skewed peer should fail loudly rather than have the policy act on it.
    #[test]
    fn heartbeat_requires_every_field() {
        let full = r#"{"type":"heartbeat","kv_cache_bytes":1,"kv_cache_budget_bytes":2,
                       "kv_cache_tokens":3,"tbt_ms":4.0,"phase":"idle","state":"idle"}"#;
        serde_json::from_str::<EngineMessage>(full).unwrap();
        for missing in [
            "kv_cache_bytes",
            "kv_cache_budget_bytes",
            "kv_cache_tokens",
            "tbt_ms",
            "phase",
            "state",
        ] {
            let v: serde_json::Value = serde_json::from_str(full).unwrap();
            let mut obj = v.as_object().unwrap().clone();
            obj.remove(missing);
            let json = serde_json::to_string(&obj).unwrap();
            assert!(
                serde_json::from_str::<EngineMessage>(&json).is_err(),
                "heartbeat without {missing} should not deserialize"
            );
        }
    }
}
