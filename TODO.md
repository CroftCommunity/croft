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

- [ ] **Adopt openmls 0.9.0 / openmls_rust_crypto 0.6.0 — ordinary work, not urgent.**
  Our pins are exact and deliberate (`=0.8.1`, `=0.5.1`, "the exact versions the
  experiments resolved"). The 0.9 line landed 2026-08-25 and brings `hpke-rs` 0.7.0,
  which pins `libcrux-sha3 =0.0.10` and retires most of `osv-scanner.toml`.

  **Why this is not an emergency** — settled 2026-08-29, see the next item. The upgrade
  is worth doing on its own merits (staying on a maintained MLS line, and shedding a
  file of exceptions), but it is an MLS stack change on a client that shipped four days
  ago, so it carries a device re-validation obligation: §12 rungs against staging
  enforce, then §13 on production. It should be planned, not slipped in.

  Version chain, measured, so nobody re-derives it:
  ```
  libcrux-sha3 0.0.10  <- hpke-rs 0.7.0  <- openmls_rust_crypto 0.6.0  <- openmls_traits 0.6.0  <- openmls 0.9.0
  ```
  There is no shorter path. `hpke-rs 0.6.1` requires `libcrux-sha3 ^0.0.8`, and under
  cargo's semver rules a `^0.0.x` requirement admits **only** `0.0.x` — so 0.0.10
  cannot be reached from anywhere in the 0.6 line, with or without `cargo update`.

## Settled

- [x] **The seven libcrux/unmaintained advisories are all unreachable in the shipped
  APK — recorded, not fixed (2026-08-29).** Found by the workspace supply-chain sweep;
  the initial report called them urgent on the strength of CVSS 8.2 plus crate-level
  reachability. Reading each advisory's own `[affected.functions]` and checking call
  sites changed the answer. Every one carries CVSS 4.0 `VC:N/VI:N/VA:H` — **no
  confidentiality impact, no integrity impact**, availability only. They are panic and
  correctness bugs, not key-compromise bugs.

  | advisory | crate | why it does not reach us |
  |---|---|---|
  | RUSTSEC-2026-0207 | libcrux-sha3 0.0.8 | affects incremental `Shake*Xof::squeeze`; hpke-rs calls one-shot `shake256::<32>/<64>` (kem.rs:154,158) |
  | RUSTSEC-2026-0208 | libcrux-sha3 0.0.8 | AVX2 → x86-64; the APK is **arm64-v8a only** |
  | RUSTSEC-2026-0212 | libcrux-secrets 0.0.5 | affects `Select::select`/`Swap::swap`; **zero call sites** in libcrux-traits or libcrux-sha3, which use only the `U8` newtype |
  | RUSTSEC-2026-0209/0210/0211 | libcrux-aesgcm 0.0.7 | unenabled optional dep via `hpke-rs-libcrux`; in the lock, in **no resolved tree on any target** |
  | RUSTSEC-2026-0124 | libcrux-chacha20poly1305 0.0.7 | same |
  | RUSTSEC-2026-0173 · RUSTSEC-2024-0384 | proc-macro-error2 · instant | unmaintained-crate notices; build-time or transitive, no fix to apply here |

  Each is a dated, reasoned, expiring entry in `osv-scanner.toml` (expiry 2026-11-29),
  per `CroftC/.claude/SUPPLY-CHAIN.md` rule 9. `osv-scanner scan source -L Cargo.lock`
  reports **No issues found** with it and 9 vulnerabilities without.

  **Two conditions that invalidate this analysis, stated because an exception that
  silently stops applying is worse than none:**
  1. **Shipping any x86_64 artifact** — a desktop shell, an emulator-targeted build —
     makes RUSTSEC-2026-0208's AVX2 path executable. Re-check on adding a target, not
     only at the expiry date.
  2. **Enabling the `hpke-rs-libcrux` feature** pulls the whole libcrux-aead tree in
     for real, and four exceptions above stop being true.

- [x] **`connect/android` needs the Gradle dependency locking this repo got.** Tracked
  in `connect/TODO.md` (landed 2026-08-29); noted here because the two Android builds
  are siblings and the fix is the same three lines.
