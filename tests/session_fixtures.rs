//! The session fixtures are inputs to the reader (T30), so they are only
//! useful while they still hold the shapes they claim. A fixture that
//! quietly stops representing its case does not fail -- it weakens the
//! test that depends on it, silently, which is the failure mode the
//! no-bypass rule exists to prevent (V71).
//!
//! So each property below is pinned here: the torn tail must stay torn,
//! the garbage line must stay garbage, the truncated result must stay
//! smaller than what it spilled. Every one traces to a real observation
//! (see the fixtures' README).

use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/session")
        .join(name)
}

fn read(name: &str) -> String {
    let text = std::fs::read_to_string(fixture(name)).unwrap_or_default();
    assert!(!text.is_empty(), "fixture {name} is missing or empty");
    text
}

/// Lines that parse as JSON, and lines that do not.
fn split_parseable(text: &str) -> (usize, usize) {
    let mut ok = 0usize;
    let mut bad = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<serde_json::Value>(line) {
            Ok(_) => ok = ok.saturating_add(1),
            Err(_) => bad = bad.saturating_add(1),
        }
    }
    (ok, bad)
}

const ALL: [&str; 5] = [
    "minimal.jsonl",
    "tool-shapes.jsonl",
    "truncated.jsonl",
    "torn-tail.jsonl",
    "weird.jsonl",
];

/// Fixtures are committed, so the source-hygiene rules bind them too.
#[test]
fn fixtures_are_ascii() {
    for name in ALL {
        let bytes = std::fs::read(fixture(name)).unwrap_or_default();
        assert!(
            bytes.iter().all(u8::is_ascii),
            "fixture {name} is not ASCII"
        );
    }
}

/// `8-4-4-4-12` hex groups -- the shape of a real session id.
fn is_uuid_shaped(token: &str) -> bool {
    let parts: Vec<&str> = token.split('-').collect();
    parts.len() == 5
        && [8usize, 4, 4, 4, 12].iter().zip(&parts).all(|(n, p)| {
            p.len() == *n && p.chars().all(|c| c.is_ascii_hexdigit())
        })
}

/// A real transcript is never committed (V45), and the hygiene guard
/// tells the two apart by looking for a UUID -- which every real session
/// carries and no hand-written fixture should.
///
/// This is the interlock: the guard stays armed inside this very
/// directory, so a real transcript pasted here brings its UUID and gets
/// caught, while these fixtures pass by being genuinely synthetic.
///
/// An earlier version of this test looked for `"sessionId": "` with a
/// space, which compact JSON never contains, so it passed while checking
/// nothing -- exactly the silent weakening this file exists to prevent.
#[test]
fn fixtures_carry_no_uuid_so_the_guard_can_tell_them_apart() {
    for name in ALL {
        let text = read(name);
        assert!(
            text.contains("\"sessionId\""),
            "fixture {name} should still look like a transcript"
        );
        let has_uuid = text
            .split(|c: char| !(c.is_ascii_hexdigit() || c == '-'))
            .any(is_uuid_shaped);
        assert!(
            !has_uuid,
            "fixture {name} carries a UUID -- is it a real capture? \
             Synthetic fixtures use short ids like \"s-min\" (V45)"
        );
    }
}

/// The happy path: every line parses.
#[test]
fn clean_fixtures_are_fully_parseable() {
    for name in ["minimal.jsonl", "tool-shapes.jsonl", "truncated.jsonl"] {
        let (ok, bad) = split_parseable(&read(name));
        assert_eq!(bad, 0, "fixture {name} has {bad} unparsable line(s)");
        assert!(ok > 0, "fixture {name} is empty");
    }
}

/// V43: the file appends live, so a reader can see a half-written final
/// line. The fixture must keep that torn tail -- and no trailing newline,
/// which is what makes it torn rather than merely short.
#[test]
fn torn_tail_stays_torn() {
    let text = read("torn-tail.jsonl");
    assert!(
        !text.ends_with('\n'),
        "torn-tail.jsonl must not end with a newline"
    );
    let (ok, bad) = split_parseable(&text);
    assert_eq!(ok, 1, "torn-tail.jsonl should have exactly 1 whole record");
    assert_eq!(bad, 1, "torn-tail.jsonl should have exactly 1 torn record");
}

/// V43: a malformed record is skipped and COUNTED, never fatal. The
/// garbage line sits mid-file, with a good record after it, so a reader
/// that aborts on the first bad line fails this rather than passing by
/// reading less.
#[test]
fn weird_keeps_one_garbage_line_with_a_record_after_it() {
    let text = read("weird.jsonl");
    let (ok, bad) = split_parseable(&text);
    assert_eq!(bad, 1, "weird.jsonl should have exactly 1 garbage line");
    assert!(ok >= 5, "weird.jsonl lost records: only {ok} parse");
    let lines: Vec<&str> = text.lines().collect();
    let garbage = lines
        .iter()
        .position(|l| serde_json::from_str::<serde_json::Value>(l).is_err());
    let last = lines.len().saturating_sub(1);
    assert!(
        garbage.is_some_and(|i| i < last),
        "the garbage line must not be last, or a reader that stops early passes"
    );
}

/// V76: the transcript records what was BILLED, not what exists on disk.
/// The spilled size must stay far larger than the retained output, or the
/// fixture stops exercising the 190x overcount it was built to catch.
#[test]
fn truncated_keeps_disk_size_far_above_billed_size() {
    let text = read("truncated.jsonl");
    let found = spills(&text);
    assert_eq!(
        found.len(),
        1,
        "truncated.jsonl must carry exactly one persisted-output record"
    );
    for (spilled, billed) in found {
        assert!(billed > 0, "the billed output must not be empty");
        assert!(
            spilled > billed.saturating_mul(1000),
            "spilled ({spilled}) must dwarf billed ({billed})"
        );
    }
}

/// Every `(spilled, billed)` pair in the text.
fn spills(text: &str) -> Vec<(u64, u64)> {
    text.lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|v| spill_vs_billed(&v))
        .collect()
}

/// `(persistedOutputSize, stdout.len())` when a record carries a spill.
fn spill_vs_billed(record: &serde_json::Value) -> Option<(u64, u64)> {
    let result = record.get("toolUseResult")?;
    let spilled = result.get("persistedOutputSize")?.as_u64()?;
    let billed = result.get("stdout")?.as_str()?.len() as u64;
    Some((spilled, billed))
}
