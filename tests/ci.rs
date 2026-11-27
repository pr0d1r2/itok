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

fn flake() -> String {
    read("flake.nix")
}

#[test]
fn the_flake_exports_intel_macos() {
    let f = flake();
    assert!(
        f.contains("\"x86_64-darwin\""),
        "flake.nix must include x86_64-darwin in its systems"
    );
    assert!(
        f.contains("packages = forAll"),
        "flake.nix must derive packages for every declared system"
    );
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
///
/// The three used to appear as three literal `nix build` lines, and this
/// test looked for those strings. They are one matrix job now, so the
/// literals are gone and the packages live in the matrix list instead --
/// the CLAIM is unchanged, only where it is written down.
///
/// It happened a SECOND time, which is why the assertions below are shaped
/// the way they are. The command was `nix build .#${{ matrix.package }}`
/// until `zizmor --persona=pedantic` asked for the matrix value to arrive
/// through `env:` rather than be pasted into the script (`V125`), and this
/// test failed on a change that altered nothing about what CI builds. Twice
/// now the guard has fired on the spelling. So it asserts the two halves of
/// the claim separately -- the matrix value reaches the step, and the build
/// consumes it -- because those are what would have to change for the claim
/// itself to stop being true.
#[test]
fn the_workflow_builds_every_feature_configuration() {
    let y = workflow();
    assert!(
        y.contains("PACKAGE: ${{ matrix.package }}"),
        "ci.yml no longer passes the matrix package to the build step"
    );
    assert!(
        y.contains(r#"nix build ".#$PACKAGE""#),
        "ci.yml no longer builds the package it was handed"
    );
    for pkg in ["default", "itok-minimal", "itok-ollama"] {
        assert!(
            y.contains(pkg),
            "ci.yml's package matrix is missing `{pkg}`"
        );
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

fn marker(text: &str, needle: &str) -> usize {
    text.find(needle).unwrap_or(usize::MAX)
}

#[test]
fn cache_work_cannot_cancel_a_passing_gate() {
    let y = workflow();
    let restore = marker(&y, "uses: actions/cache/restore@");
    let check = marker(&y, "run: nix develop --command hk check --all --check");
    let save = marker(&y, "uses: actions/cache/save@");
    let ordered = restore < check && check < save;
    let bounded = y[restore..check].matches("timeout-minutes: 3").count() == 1
        && y[save..save + 500].matches("timeout-minutes: 3").count() == 1;
    let best_effort = y[restore..check].contains("continue-on-error: true")
        && y[save..].contains("continue-on-error: true");
    assert!(ordered && bounded && best_effort);
    assert!(y.contains("if: steps.cargo-cache.outputs.cache-hit != 'true'"));
    assert!(
        y[check.saturating_sub(200)..check].contains("timeout-minutes: 15")
    );
}

/// B49: the gate's 15-minute bound must time the checks, not the shell
/// build. The shell is materialised by an earlier, separately bounded step.
#[test]
fn the_shell_build_has_its_own_bound() {
    let y = workflow();
    let shell = marker(&y, "run: nix develop --command true");
    let check = marker(&y, "run: nix develop --command hk check --all --check");
    assert!(shell < check);
    assert!(y[shell.saturating_sub(40)..shell].contains("timeout-minutes: 25"));
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

/// The newest released heading in CHANGELOG.md (skipping `[Unreleased]`).
fn newest_released() -> String {
    read("CHANGELOG.md")
        .lines()
        .filter_map(|l| l.strip_prefix("## ["))
        .filter_map(|l| l.split(']').next())
        .find(|v| v.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .unwrap_or_default()
        .to_string()
}

/// #101: an automated bump set `0.8.0` with no tag or publish, twice. The
/// manifest may carry the newest released version or a newer one only if the
/// CHANGELOG has a heading for it; it must never run ahead of the ladder.
#[test]
fn the_manifest_version_is_a_released_changelog_heading() {
    let want = env!("CARGO_PKG_VERSION");
    assert_eq!(
        want,
        newest_released(),
        "Cargo.toml version must equal the newest released CHANGELOG heading (V70)"
    );
}

/// `major.minor` of a version string, ignoring the patch and any suffix.
fn rung_of(v: &str) -> Option<(u64, u64)> {
    let mut parts = v.split(['.', '-']).map(str::parse::<u64>);
    Some((parts.next()?.ok()?, parts.next()?.ok()?))
}

/// The rungs of the CHANGELOG's `## Versioning` table, in order.
fn ladder() -> Vec<(u64, u64)> {
    let text = read("CHANGELOG.md");
    text.lines()
        .skip_while(|l| *l != "## Versioning")
        .skip(1)
        .take_while(|l| !l.starts_with("## "))
        .filter_map(|l| l.strip_prefix("| `"))
        .filter_map(|l| l.split('`').next())
        .filter_map(rung_of)
        .collect()
}

/// Every `major.minor` a released CHANGELOG heading has reached.
fn released_rungs() -> Vec<(u64, u64)> {
    read("CHANGELOG.md")
        .lines()
        .filter_map(|l| l.strip_prefix("## ["))
        .filter_map(|l| l.split(']').next())
        .filter_map(rung_of)
        .collect()
}

/// V70: the ladder is "a number you cannot skip". The heading test above
/// cannot see a skip -- a release PR that adds `[0.8.0]` and bumps to
/// `0.8.0` is self-consistent, and that is exactly what PR #108 did while
/// `0.4`, `0.6` and `0.7` were open. So every rung BELOW the manifest's own
/// must already have a released heading.
#[test]
fn the_manifest_version_skips_no_open_rung() {
    let current = rung_of(env!("CARGO_PKG_VERSION"));
    let reached = released_rungs();
    let ladder = ladder();
    assert!(
        ladder.len() >= 2,
        "CHANGELOG.md's Versioning table is unreadable"
    );
    let skipped: Vec<_> = ladder
        .into_iter()
        .filter(|r| Some(*r) < current && !reached.contains(r))
        .collect();
    assert!(
        skipped.is_empty(),
        "Cargo.toml's version skips open rungs {skipped:?} (V70)"
    );
}

/// A slow cachix upload must not cancel a nix-build job whose build passed:
/// the action's post-step push is off, and the push is a separate,
/// best-effort, bounded step after the build.
#[test]
fn cachix_push_cannot_cancel_a_passing_build() {
    let y = workflow();
    let job = marker(&y, "  nix-build:");
    let job_y = &y[job..];
    assert!(job_y.contains("skipPush: true"));
    let build = marker(job_y, "run: nix build");
    let push = marker(job_y, "cachix push");
    assert!(build < push);
    assert!(job_y[..build].contains("timeout-minutes: 10"));
    let tail = &job_y[build..push];
    assert!(tail.contains("continue-on-error: true"));
    assert!(tail.contains("timeout-minutes: 5"));
    assert!(job_y.contains("timeout-minutes: 25"));
}
