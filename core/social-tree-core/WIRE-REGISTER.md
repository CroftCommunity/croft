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
| `GroupState` | **0x04** (`model::GROUP_STATE_WIRE_VERSION`) | `model.rs` (`to_bytes`/`from_bytes`) | v3 = v2 + the §7.6.4 standing-ceiling set (banned lineages, count ‖ 32 each, appended after fork status) — P4's admission facts need bans readable at position; v1/v2 refused with a rebuild demand |
| Replay comparator | **v2** (`model::MERGE_CMP_VERSION`) | `model.rs` (`merge_cmp`) | lamport → content address; v1's `author_device` key was party-privileging (G1) — stores stamp the version, `needs_rebuild` (adapter) detects staleness |
| `Versioned<T>` wrapper | 0x01 | `model.rs` | generic value tagging |
| Governance-fact payloads | per `AssertionType` | `model.rs` payload structs | byte layouts documented per type; `Resolution` (0x000C) = the ordered pair (32 ‖ 32); `MembershipRemove` (0x0003) = subject(32) ‖ §7.6.4 kind(1) — kindless refused; `TokenIssuance` (0x000D) = token(32) ‖ lineage(32); `TokenRevocation` (0x000E) = token(32); `Admission` (0x000F) = event(32) ‖ lineage(32) ‖ token(32) ‖ frontier(8) |
