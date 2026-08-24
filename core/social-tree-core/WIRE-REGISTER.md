# Wire-format version register (O2)

Every serialized artifact this crate reads or writes opens with a version byte,
and every decoder refuses unknown versions loudly — a stale store demands a
rebuild, never a reinterpretation. One row per artifact; bump = new row, with
the old row's disposition stated. The `[gates-release]` wire-freeze (Part 2,
Appendix B) will pin final encodings; until then these are the experiment-grade
encodings the corpus proves against, and this register is where the freeze
finds its sockets.

| Artifact | Version | Layout home | Notes |
|---|---|---|---|
| `AssertionEnvelope` canonical bytes | **0x02** (`model::ENVELOPE_WIRE_VERSION`) | `model.rs` (`canonical_bytes`) | v1 carried a signed wall-clock field — dropped (O9, Part 1 §2.0.1); v1 is refused by the decoder |
| `GroupState` | **0x02** (`model::GROUP_STATE_WIRE_VERSION`) | `model.rs` (`to_bytes`/`from_bytes`) | v2 = five thresholds + the set-valued contested entries (E108); v1 refused with a rebuild demand |
| Replay comparator | **v2** (`model::MERGE_CMP_VERSION`) | `model.rs` (`merge_cmp`) | lamport → content address; v1's `author_device` key was party-privileging (G1) — stores stamp the version, `needs_rebuild` (adapter) detects staleness |
| `Versioned<T>` wrapper | 0x01 | `model.rs` | generic value tagging |
| Governance-fact payloads | per `AssertionType` | `model.rs` payload structs | byte layouts documented per type; `Resolution` (0x000C) payload = the ordered pair (32 ‖ 32) |
