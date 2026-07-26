//! V89's property, kept from being lost silently (T71).
//!
//! itok is safe to run concurrently, and today that is FREE: the shipped
//! binary writes nothing, mutates no env and no cwd, has no `static mut`,
//! and forbids `unsafe`. A pure function of its inputs cannot race.
//!
//! Free is not the same as guaranteed. `guard` (V52) is one process per
//! hook call and hooks fire concurrently, so many itok processes at once is
//! the NORMAL case, not an edge one. And V89's premise stopped being purely
//! theoretical when T89 put real threads inside the fleet probe.
//!
//! So this asserts the two things that would break first, at the process
//! boundary where a user actually meets them: concurrent runs agree, and
//! nothing outside `target/` is written. Reading the source for temp-dir
//! handling establishes nothing -- B5 passed exactly that reading and still
//! raced (V68: hermeticity is proven by running parallel, not argued).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;

const DIR: &str = env!("CARGO_MANIFEST_DIR");

/// How many at once. Enough to interleave on any machine the gate runs on,
/// small enough that the suite stays fast when nextest is already running
/// tests in parallel around it.
const N: usize = 8;

/// One run's observable result: what a caller sees, and nothing else.
#[derive(PartialEq, Eq, Debug)]
struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn run(args: &[&str]) -> Run {
    let bin = env!("CARGO_BIN_EXE_itok");
    let out = Command::new(bin)
        .args(args)
        .current_dir(DIR)
        .output()
        .expect("itok runs");
    Run {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// Run the same command in N processes at once and collect every result.
fn race(args: &'static [&'static str]) -> Vec<Run> {
    let handles: Vec<_> =
        (0..N).map(|_| thread::spawn(move || run(args))).collect();
    handles
        .into_iter()
        .map(|h| h.join().expect("no panic"))
        .collect()
}

/// Concurrent runs of a report verb produce IDENTICAL output.
///
/// The verb is `estimate -s` on the dummy tier: it reads the git-tracked
/// fileset and needs no optional feature, so this same assertion holds on
/// the `--no-default-features` axis where `--bpe` does not exist.
#[test]
fn concurrent_runs_agree() {
    let results = race(&["estimate", "-s"]);
    let first = results.first().expect("N > 0");
    for (i, r) in results.iter().enumerate() {
        assert_eq!(r, first, "run {i} disagreed with run 0");
    }
    assert_eq!(first.code, Some(0), "report-only, exit 0 (V5)");
    assert!(first.stdout.contains("itok"), "produced a real report");
}

/// A gate verb races too. `check` reads `.context-limits` and decides, so
/// it is the one whose disagreement would matter most: a gate that varies
/// per run is not a gate (V5).
#[test]
fn a_gate_verb_agrees_under_load() {
    let results = race(&["check"]);
    let first = results.first().expect("N > 0");
    for (i, r) in results.iter().enumerate() {
        assert_eq!(r.code, first.code, "run {i} reached a different verdict");
        assert_eq!(r.stdout, first.stdout, "run {i} reported differently");
    }
}

/// N concurrent runs write NOTHING outside `target/`.
///
/// The whole tree is fingerprinted before and after -- path plus length --
/// so a created file, a deleted one, or a changed one all show up. `target/`
/// is excluded because cargo owns it; `.git` because git does.
#[test]
fn concurrency_writes_nothing_outside_target() {
    let before = fingerprint();
    let _ = race(&["estimate", "-s"]);
    let _ = race(&["check"]);
    let after = fingerprint();
    assert_eq!(
        changed(&before, &after),
        Vec::<String>::new(),
        "itok wrote outside target/ -- V89's free property is gone"
    );
}

/// Paths whose presence or size differs between two fingerprints.
fn changed(
    a: &BTreeMap<String, u64>,
    b: &BTreeMap<String, u64>,
) -> Vec<String> {
    let mut out: Vec<String> = a
        .iter()
        .filter(|(p, len)| b.get(*p) != Some(len))
        .map(|(p, _)| p.clone())
        .collect();
    out.extend(b.keys().filter(|p| !a.contains_key(*p)).cloned());
    out.sort();
    out.dedup();
    out
}

/// Every file under the repo, path -> length, skipping what other tools own.
fn fingerprint() -> BTreeMap<String, u64> {
    let mut out = BTreeMap::new();
    let mut stack = vec![PathBuf::from(DIR)];
    while let Some(dir) = stack.pop() {
        visit(&dir, &mut stack, &mut out);
    }
    out
}

/// One directory: record its files, queue its subdirectories.
fn visit(
    dir: &Path,
    stack: &mut Vec<PathBuf>,
    out: &mut BTreeMap<String, u64>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        if skipped(&path) {
            continue;
        }
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            stack.push(path);
        } else {
            let len = e.metadata().map(|m| m.len()).unwrap_or(0);
            out.insert(path.to_string_lossy().into_owned(), len);
        }
    }
}

/// Directories another tool owns and churns on its own schedule.
fn skipped(path: &Path) -> bool {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    matches!(name.as_deref(), Some("target" | ".git" | ".direnv" | ".jj"))
}

/// The detector detects. Asserted on the comparison itself rather than by
/// planting a file in the repo: another test fingerprints the tree
/// concurrently, so a real write would make THAT test fail instead -- a
/// flake introduced to prove a guard is a bad trade (B5/V68).
#[test]
fn the_write_detector_sees_every_kind_of_change() {
    let base: BTreeMap<String, u64> =
        [("a".to_owned(), 1u64), ("b".to_owned(), 2u64)].into();
    assert_eq!(changed(&base, &base), Vec::<String>::new(), "no change");

    let mut added = base.clone();
    added.insert("c".to_owned(), 3);
    assert_eq!(changed(&base, &added), vec!["c"], "a created file");

    let mut removed = base.clone();
    removed.remove("b");
    assert_eq!(changed(&base, &removed), vec!["b"], "a deleted file");

    let mut resized = base.clone();
    resized.insert("a".to_owned(), 99);
    assert_eq!(changed(&base, &resized), vec!["a"], "a rewritten file");
}

/// The fingerprint really walks the tree, and really skips what cargo owns
/// -- otherwise the guard above would pass by measuring nothing.
#[test]
fn the_fingerprint_covers_the_tree_and_skips_target() {
    let fp = fingerprint();
    assert!(fp.len() > 20, "walked the tree: {} entries", fp.len());
    assert!(
        fp.keys().any(|p| p.ends_with("SPEC.md")),
        "a known tracked file is present"
    );
    assert!(
        !fp.keys().any(|p| p.contains("/target/")),
        "cargo's directory is excluded"
    );
}
