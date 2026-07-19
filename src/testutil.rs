//! Shared test helpers that keep the git-command tests LAYOUT-AGNOSTIC
//! (V37): the enclosing repo root and this crate's path prefix come from
//! git at RUNTIME, so one test passes both at `<repo>/crates/itok` and when
//! the crate is the extracted repo root. Hardcoding `parent().parent()` as
//! the root or a `crates/itok/...` pathspec couples a test to the monorepo
//! layout and breaks the extraction rehearsal (B2/T11).

use std::process::Command;

const DIR: &str = env!("CARGO_MANIFEST_DIR");

/// The enclosing git repo's root (`git rev-parse --show-toplevel`): the
/// monorepo root in-tree, the crate dir once extracted.
pub(crate) fn repo_root() -> String {
    git(&["rev-parse", "--show-toplevel"])
}

/// A repo-root-relative path to one of this crate's own files:
/// `crates/itok/SPEC.md` in the monorepo, `SPEC.md` when extracted
/// (`git rev-parse --show-prefix` yields the crate's prefix, empty at the
/// repo root).
pub(crate) fn crate_path(rel: &str) -> String {
    format!("{}{rel}", git(&["rev-parse", "--show-prefix"]))
}

fn git(args: &[&str]) -> String {
    Command::new("git")
        .arg("-C")
        .arg(DIR)
        .args(args)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}
