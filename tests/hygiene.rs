//! Source hygiene the CONSTRAINTS section states but nothing enforced.
//!
//! Deliberately a TEST rather than a linter step: it needs no external
//! binary, so it holds wherever cargo runs -- the dev shell, a bare rustup
//! CI runner, a contributor's laptop -- and cannot be skipped by not
//! having a tool installed (V65). Layout-agnostic (V37).

use std::path::{Path, PathBuf};

/// Every `.rs` file under `src/` and `tests/`.
fn rust_sources() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut out = Vec::new();
    for dir in ["src", "tests"] {
        collect(&root.join(dir), &mut out);
    }
    assert!(!out.is_empty(), "found no sources to check");
    out
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// ASCII-only source (a stated constraint). Two reasons, both
/// load-bearing.
///
/// Trojan Source (CVE-2021-42574): bidirectional control characters can
/// make source read to a human as something other than what it compiles
/// to. And a token tool has a second stake -- non-ASCII costs more tokens
/// per character in every tokenizer itok ships, so ASCII source is itok
/// taking its own advice (V15).
///
/// Prose files are exempt on purpose: SPEC.md is written in a symbolic
/// shorthand, and README renders arrows. The constraint is on SOURCE.
#[test]
fn source_is_ascii_only() {
    for path in rust_sources() {
        let bytes = std::fs::read(&path).unwrap_or_default();
        let bad = bytes.iter().position(|b| !b.is_ascii());
        assert!(
            bad.is_none(),
            "non-ASCII byte at offset {} in {}",
            bad.unwrap_or(0),
            path.display()
        );
    }
}

/// A stray debug macro is a debugging aid that shipped. clippy's
/// `dbg_macro` is denied for `src/`, but tests are a target too and the
/// habit spreads.
///
/// The needle is assembled from two halves so that THIS file does not
/// match itself -- the first version of this test failed on its own
/// source, which is the correct behaviour from a wrong spelling.
#[test]
fn no_debug_macros_left_behind() {
    let needle = concat!("dbg", "!(");
    for path in rust_sources() {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        for (n, line) in text.lines().enumerate() {
            assert!(
                !line.contains(needle),
                "debug macro left in {}:{}",
                path.display(),
                n.saturating_add(1)
            );
        }
    }
}
