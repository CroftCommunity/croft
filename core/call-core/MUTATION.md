# Mutation testing — call-core

Rules engines are where a green suite most easily hides a hole, so this crate
carries its baseline from the commit that created it.

    cargo mutants -p call-core --no-shuffle

**R1 baseline, 2026-09-14:** 27 mutants — **22 caught, 5 unviable, 0 missed**
(78 s). Nothing to triage: no survivor, equivalent or otherwise. The three
boundary points on the re-mint margin and the per-reason word tables are what
kill the operator and arm swaps a single-point suite would have let through.

Discipline (workspace standing rules): commit green BEFORE mutating; read the
survivors, don't chase the score; a timeout is a kill. One crate-specific
gotcha: the matrix walker locates `docs/ENFORCEMENT-SCENARIOS.md` by layout
(two levels above the manifest), not by walking to `.git` — `cargo mutants`
runs the suite in a copy of the tree with no VCS directory, and the first run
here reported "cargo test failed in an unmutated tree" for exactly that reason.
