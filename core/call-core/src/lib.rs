//! # call-core
//!
//! The call pond's decision rules: whether an attach presents a camping pass,
//! what each camp-mint outcome does to the camp, which proof a dial presents,
//! what each mint outcome does to the dial, and what a dial may do to the
//! endpoint's bound relay token. A port of the shipped Kotlin
//! (`android/app/.../CampAdmission.kt`, `DialAdmission.kt`), graded by the same
//! `docs/ENFORCEMENT-SCENARIOS.md` rows (`RUST:` pins beside `PIN:`).
//!
//! Pure by contract: no I/O, no async, no clock — the time is a parameter
//! (`now_ms`), enforced by `clippy.toml`'s disallowed methods and the wasm32
//! CI arm. The core knows what answers admission can give, not how to get one:
//! fetching stays with the shell (the `caps/` engine is not ported).
#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod camp;
pub mod dial;
pub mod model;
