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

/// A harness transcript must never be committed (V45). `.gitignore`
/// catches the common path, but a rename defeats a filename pattern --
/// so this checks CONTENT, which a rename cannot change.
///
/// The signature is deliberately narrow: a `sessionId` field beside the
/// per-turn token accounting only a real transcript carries. itok's own
/// sources discuss those field names in prose, so the test reads TRACKED
/// DATA FILES and skips `.rs` and `.md`, where the words are the subject
/// rather than the payload.
/// Repo-relative paths of tracked files that could carry a payload.
/// `.rs` and `.md` are skipped: itok's own sources and spec DISCUSS these
/// field names in prose, where the words are the subject rather than the
/// content.
fn tracked_data_files(root: &Path) -> Vec<String> {
    let Ok(out) = std::process::Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root)
        .output()
    else {
        return Vec::new(); // no git here; the gate runs this elsewhere
    };
    String::from_utf8_lossy(&out.stdout)
        .split('\0')
        .filter(|s| !s.is_empty())
        .filter(|s| !(s.ends_with(".rs") || s.ends_with(".md")))
        .map(str::to_owned)
        .collect()
}

/// True when the text carries a UUID-shaped token, which every real
/// harness session has and a hand-written fixture does not.
///
/// This is what separates a CAPTURE from a synthetic fixture, and it is
/// deliberately intrinsic to the content rather than based on location:
/// the likeliest place to drop a real transcript is the fixture
/// directory, so exempting that directory would disarm the guard exactly
/// where it is most needed.
fn has_a_uuid(text: &str) -> bool {
    text.split(|c: char| !(c.is_ascii_hexdigit() || c == '-'))
        .any(is_uuid_shaped)
}

/// `8-4-4-4-12` hex groups.
fn is_uuid_shaped(token: &str) -> bool {
    let parts: Vec<&str> = token.split('-').collect();
    parts.len() == 5
        && [8usize, 4, 4, 4, 12].iter().zip(&parts).all(|(n, p)| {
            p.len() == *n && p.chars().all(|c| c.is_ascii_hexdigit())
        })
}

/// A `sessionId` beside the per-turn token accounting -- the pair only a
/// real transcript carries -- plus a UUID, which is what makes it a
/// capture rather than a fixture faithfully imitating the shape.
fn looks_like_a_transcript(text: &str) -> bool {
    text.contains("\"sessionId\"")
        && text.contains("\"cache_read_input_tokens\"")
        && has_a_uuid(text)
}

/// A harness transcript must never be committed (V45). `.gitignore`
/// catches the common path, but a rename defeats a filename pattern --
/// so this checks CONTENT, which a rename cannot change.
#[test]
fn no_harness_transcript_is_committed() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for rel in tracked_data_files(root) {
        let text = std::fs::read_to_string(root.join(&rel)).unwrap_or_default();
        assert!(
            !looks_like_a_transcript(&text),
            "{rel} looks like a harness transcript -- it carries real \
             conversation content and must not be committed (V45). Use a \
             synthetic fixture instead."
        );
    }
}
