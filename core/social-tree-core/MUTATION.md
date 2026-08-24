# Mutation testing — the re-baseline and the cross-repo recipe

"Mutation-vetted" does not transfer across a re-cut (E117's vet, R6): the
pre-extraction ledger was bound to the old module map, and a git-pinned
dependency cannot be mutated in place. So the baseline is re-earned HERE and
the corpus-side sweep gets a recipe.

## In-crate re-baseline

    cargo mutants -p social-tree-core -j 4 --timeout 120

Discipline (the workspace's standing rules): commit green BEFORE mutating;
read the survivors, don't chase the score — triage each as *equivalent*
(state the argument) or *real gap* (write the killer); a timeout is a kill.
Results land in `mutants.out/`; the durable record is the triage summary in
the phase's evidence ledger, not the raw directory.

## Corpus-side (cross-repo) sweeps — the `[patch]` recipe

The discovery corpus (`local_storage_projection`, croft-chat) consumes this
crate pinned by commit, so a corpus-side harness cannot mutate the dependency
it fetched. Point the consumer at a mutable checkout instead — in the
CONSUMER workspace's `Cargo.toml`:

    [patch."file:///Users/…/croft"]        # or the github-personal URL post-push
    social-tree-core = { path = "/Users/…/croft/core/social-tree-core" }

Then the X3-style harness (`local_storage_projection/x3_cross_package_harness.py`)
applies mutant diffs to `core/social-tree-core/src/**` in the croft checkout,
runs the consumer suite, and restores with `git checkout HEAD -- <path>` —
which is also why the checkout must be committed-green before the sweep
starts. Remove the `[patch]` block when the sweep ends; a lingering patch is
a silent unpin.
