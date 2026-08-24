# social-tree-core

The Drystone social-tree substrate — groups, membership, standing, the
governance fold and its projections — as a pure crate: no storage, no I/O, no
async, no clock. The foundation the per-pond feature cores (`call-core`,
`feed-core`, the coming `chat-core`) stand on; **not a pond itself**
(../../docs/ADR-0002-core-layering.md).

Extracted at E117 Phase 2 from the discovery corpus's mutation-vetted
`local_storage_projection` experiment, which remains the redb **adapter** and
consumes this crate pinned by commit. The behavior evidence (C-series, the
E108 pins, the S-series) lives with that corpus; this crate carries the pure
re-statements of the load-bearing pins (`tests/fold_pins.rs`) so the fold's
properties are enforced here even when the corpus is far away.

Doc coverage rides the ratchet: `#![warn(missing_docs)]` today (the
transplanted experiment surface predates the discipline), deny when clean.
