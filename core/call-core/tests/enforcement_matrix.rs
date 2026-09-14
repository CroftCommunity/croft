//! The Rust half of the enforcement matrix walker (child plan R1, D5 option (a)).
//!
//! `docs/ENFORCEMENT-SCENARIOS.md` names, per client-posture row, the test that
//! pins it. A row may carry a Kotlin pin (``PIN:File.kt::`test name` ``, checked by
//! `EnforcementMatrixTest`) and a Rust pin (`RUST:<path>::<fn>`, path relative to
//! `core/call-core`, checked here). Each walker checks only its own side.
//!
//! This is R1's anti-dead-code gate: until R3 wires a caller, the matrix is the
//! only consumer the rules have, so the rules must be reachable from it.
//! One-sided rows (a `PIN:` with no `RUST:`) are allowed, counted, and the count
//! may only shrink — incomplete coverage stays visible instead of silent.

use std::fs;
use std::path::{Path, PathBuf};

/// Rows that name a Kotlin pin and no Rust pin. Lower this number when a row
/// gains its `RUST:`; never raise it.
const ONE_SIDED_ROWS: usize = 13;

/// The repo root is two levels above this crate (`core/call-core`). Fixed by
/// layout rather than found by walking to `.git`: `cargo mutants` runs the
/// suite in a copy of the tree with no VCS directory, and a walker that cannot
/// find its document there reports every mutant as untestable.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
        .expect("core/call-core sits two levels below the repo root")
}

fn matrix() -> String {
    let path = repo_root().join("docs/ENFORCEMENT-SCENARIOS.md");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()))
}

/// Every `RUST:<path>::<fn>` token in the table ROWS, as (path, fn) pairs.
/// Prose may name the syntax (the grammar paragraph does); only rows carry pins,
/// the same scope `EnforcementMatrixTest` gives its GAP rule.
fn rust_pins(text: &str) -> Vec<(String, String)> {
    table_rows(text)
        .flat_map(|row| row.match_indices("RUST:").map(move |(i, _)| (row, i)))
        .map(|(row, i)| {
            let rest = &row[i + "RUST:".len()..];
            let token: String = rest
                .chars()
                .take_while(|c| !c.is_whitespace() && *c != '|')
                .collect();
            let (path, name) = token
                .split_once("::")
                .unwrap_or_else(|| panic!("malformed RUST pin `{token}` — want RUST:<path>::<fn>"));
            (path.to_string(), name.to_string())
        })
        .collect()
}

fn table_rows(text: &str) -> impl Iterator<Item = &str> {
    text.lines().filter(|l| l.trim_start().starts_with('|'))
}

#[test]
fn the_matrix_carries_rust_pins() {
    let pins = rust_pins(&matrix());
    assert!(
        !pins.is_empty(),
        "the matrix names no RUST: pins — the rules are unreachable from the document that defines them"
    );
}

#[test]
fn every_rust_pin_names_a_test_that_exists() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let failures: Vec<String> = rust_pins(&matrix())
        .into_iter()
        .filter_map(|(path, name)| {
            let file = crate_root.join(&path);
            let Ok(src) = fs::read_to_string(&file) else {
                return Some(format!("MISSING FILE {path} (for `{name}`)"));
            };
            let defined = src.contains(&format!("fn {name}("));
            (!defined).then(|| format!("MISSING TEST `{name}` in {path}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn one_sided_rows_only_shrink() {
    let text = matrix();
    let one_sided: Vec<&str> = table_rows(&text)
        .filter(|row| row.contains("PIN:") && !row.contains("RUST:"))
        .collect();
    assert_eq!(
        one_sided.len(),
        ONE_SIDED_ROWS,
        "rows pinned on the Kotlin side only: {} (pinned count {ONE_SIDED_ROWS}; lower the constant when a row gains its RUST: pin, never raise it)\n{}",
        one_sided.len(),
        one_sided.join("\n")
    );
}
