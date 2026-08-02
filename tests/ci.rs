//! The gate is defined ONCE, in `hk.pkl` (V64), and reached three ways:
//! a local `pre-commit` hook, a local `pre-push` hook, and `hk check` in
//! `.github/workflows/ci.yml`. These tests freeze that arrangement: the
//! ops must stay in the single definition, the workflow must delegate
//! rather than restate, and the orchestration only a runner can provide
//! must stay present. Layout-agnostic (V37): both files sit at the crate
//! root in either layout, so CARGO_MANIFEST_DIR finds them.

use std::path::Path;

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(p).unwrap_or_default()
}

fn gate() -> String {
    read("hk.pkl")
}

fn workflow() -> String {
    read(".github/workflows/ci.yml")
}

#[test]
fn the_gate_defines_the_standard_ops() {
    let g = gate();
    for op in [
        "cargo fmt --check",
        "cargo clippy",
        "cargo nextest run",
        "cargo test --doc", // nextest skips doctests; this closes that gap
        "--features ollama", // V38: the cassette-replayed network axis
        "--fail-under-lines 98", // itok's OWN standalone coverage floor (B4)
        "itok -- check",    // V14: itok gates itself
    ] {
        assert!(g.contains(op), "hk.pkl missing `{op}`");
    }
}

#[test]
fn the_gate_covers_every_phase() {
    let g = gate();
    for hook in ["[\"pre-commit\"]", "[\"pre-push\"]", "[\"check\"]"] {
        assert!(g.contains(hook), "hk.pkl missing hook {hook}");
    }
}

#[test]
fn the_pkl_schema_is_vendored() {
    // V12's reasoning, applied to the gate config: offline-first is the
    // default BUILD, not merely the default flag. A `package://` amends
    // would fetch the schema over the network at eval time.
    assert!(
        gate().contains("amends \"pkl/Config.pkl\""),
        "hk.pkl must amend the vendored schema, not a package:// URL"
    );
    assert!(!read("pkl/Config.pkl").is_empty(), "pkl/Config.pkl missing");
}

#[test]
fn the_workflow_delegates_to_the_gate() {
    let y = workflow();
    assert!(y.contains("hk check"), "ci.yml must run `hk check`");
    // Through the dev shell, never a separate install: hk is a LOCAL
    // tool, and reaching it via `nix develop` is what keeps CI and a
    // laptop on the same pinned versions (V75).
    assert!(
        y.contains("nix develop --command hk check"),
        "ci.yml must reach hk through the dev shell, not install it"
    );
}

/// V23/V13's zero-dependency core is only a claim until something builds
/// it; CI building all three feature configurations is what makes it one.
#[test]
fn the_workflow_builds_every_feature_configuration() {
    let y = workflow();
    for pkg in [
        "nix build .#default",
        "nix build .#itok-minimal",
        "nix build .#itok-ollama",
    ] {
        assert!(y.contains(pkg), "ci.yml missing `{pkg}`");
    }
}

#[test]
fn the_workflow_restates_no_op() {
    // The whole point of V64: one definition. A `run: cargo ...` step here
    // is a second copy, and a second copy is what drifted in B4.
    for line in workflow().lines() {
        let t = line.trim_start().trim_start_matches("- ").trim_start();
        assert!(
            !t.starts_with("run: cargo"),
            "ci.yml runs cargo directly (`{t}`) -- ops belong in hk.pkl"
        );
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

/// V75 amends V31: nix is now this crate's OWN toolchain rather than a
/// host dependency, so CI uses the dev shell. What survives from V31 is
/// the part that still matters -- no tool gets a second, drifting install
/// path. That the runner is hk, reached through the shell, is asserted
/// positively by `the_workflow_delegates_to_the_gate`.
#[test]
fn no_second_install_path() {
    let y = workflow();
    for tool in ["install-action", "install hk", "actionlint_"] {
        assert!(
            !y.contains(tool),
            "ci.yml installs `{tool}` separately -- tools come from the pinned shell (V75)"
        );
    }
}
