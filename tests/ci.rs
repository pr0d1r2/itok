//! The public CI workflow (`.github/workflows/ci.yml`) travels with the
//! crate (V31/V39) and reproduces the STANDARD gate on plain rustup -- no
//! nix, no uow, no host bin. This freezes its contract: dropping a step,
//! the ollama axis, the coverage floor, or the full-history checkout fails
//! HERE, in the monorepo, before it ships. Layout-agnostic (V37): the file
//! sits at the crate root in both layouts, so CARGO_MANIFEST_DIR finds it.

use std::path::Path;

fn workflow() -> String {
    let p =
        Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/ci.yml");
    std::fs::read_to_string(p).unwrap_or_default()
}

#[test]
fn reproduces_the_standard_gate() {
    let y = workflow();
    for step in [
        "cargo fmt --check",
        "cargo clippy",
        "cargo test",
        "--features ollama", // V38: the cassette-replayed network axis
        "--fail-under-lines 99", // the coverage floor, matching the monorepo
    ] {
        assert!(y.contains(step), "ci.yml missing `{step}`");
    }
}

#[test]
fn fetches_full_history() {
    // The diff/show/log tests read HEAD~n; a shallow clone fails them (B3).
    assert!(
        workflow().contains("fetch-depth: 0"),
        "ci.yml needs fetch-depth: 0"
    );
}

#[test]
fn is_bare_rust_no_host_runner() {
    // V31: cargo-native on rustup, not the monorepo's nix/uow gate.
    let y = workflow();
    assert!(y.contains("rust-toolchain"), "must use rustup");
    assert!(!y.contains("nix develop"), "no nix devshell");
    assert!(!y.contains("uow run"), "no uow runner");
}
