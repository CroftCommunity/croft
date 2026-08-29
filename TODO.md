# croft — repo open items

> Known work only — items whose shape is already decided, and which may therefore be
> proposed as work. Anything still an open question (decide / verify / investigate /
> reconcile) belongs in the backlog of record, `discovery/alpha/ROADMAP_TODO.md`,
> however small or operational it is. Tracking scheme: `CroftC/.claude/TRACKING.md`;
> the two piles and why: its § "Two piles". Cross-reference E-numbers where an item
> here implements a backlog row.

Client-side work local to this repo. Dated plans live in `plans/`; device-run evidence
in `ops/RUNBOOK-*.md` and `sessions/`.

## Open

- [ ] **Bump the libcrux crypto crates — three CVSS 8.2 advisories are in the shipped
  MLS key layer.** Found by the workspace supply-chain sweep, 2026-08-29
  (`CroftC/.claude/SUPPLY-CHAIN.md`). v0.5.0 — the released client, versionCode 6,
  running on both test phones — carries:

  | crate | version | advisories | fixed in |
  |---|---|---|---|
  | `libcrux-sha3` | 0.0.8 | RUSTSEC-2026-0207, RUSTSEC-2026-0208 (CVSS 8.2 each) | 0.0.10 |
  | `libcrux-secrets` | 0.0.5 | RUSTSEC-2026-0212 (CVSS 8.2) | 0.0.6 |

  **Reachability is confirmed, not assumed** — both resolve in the *normal* dependency
  path, so this is not a dev-only finding:

  ```
  libcrux-sha3 v0.0.8
  └── hpke-rs v0.6.1
      └── openmls_rust_crypto v0.5.1
          └── keylayer-openmls v0.1.0 (ports/keylayer-openmls)
  ```

  Both are transitive under `hpke-rs`/`openmls_rust_crypto`, so the bump likely means
  moving those rather than the libcrux crates directly — check whether a newer
  `openmls_rust_crypto` pulls the fixed versions before forcing anything with a patch
  section. Re-run `osv-scanner scan source -L Cargo.lock` after, and re-check
  reachability with `cargo tree -i <crate> --edges normal`.

- [ ] **Run the `--target all` reachability pass on the remaining libcrux advisories.**
  `libcrux-aesgcm` 0.0.7 (RUSTSEC-2026-0209/0210/0211) and
  `libcrux-chacha20poly1305` 0.0.7 (RUSTSEC-2026-0124, CVSS 8.2) are in `Cargo.lock`
  but did **not** resolve in the default-target normal tree. That means unproven in
  both directions: they are not confirmed reachable, and equally not confirmed safe.
  `cargo tree -i <crate> --edges normal --target all` settles it. Whichever way it
  lands, record the answer — an unproven advisory that is quietly dropped reads later
  as one that was cleared.

- [ ] **`libcrux-aesgcm` has no fixed version upstream.** If the pass above shows it
  reachable, it needs a dated, reasoned entry in `osv-scanner.toml` rather than
  silence — `SUPPLY-CHAIN.md` rule 9. If it shows unreachable, it needs the note
  above instead.

- [ ] **`connect/android` still needs the Gradle dependency locking this repo got.**
  Tracked in `connect/TODO.md`; noted here because the two Android builds are
  siblings and the fix is the same three lines.
