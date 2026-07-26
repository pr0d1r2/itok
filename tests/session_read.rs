//! T30: the session reader, against the fixtures T30b pinned.
//!
//! One test per invariant the reader has to hold. The fixtures exist
//! because characterising a real transcript proved each shape occurs, so
//! these are regression tests for observed reality rather than imagined
//! edge cases.

#![cfg(feature = "session")]

use itok::session::{claude_code, Source};
use std::path::Path;

fn read(name: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/session")
        .join(name);
    let text = std::fs::read_to_string(p).unwrap_or_default();
    assert!(!text.is_empty(), "fixture {name} missing or empty");
    text
}

/// V44: `usage` is present on every assistant record, so the total is
/// EXACT. The reader must carry the four counts through unchanged.
#[test]
fn usage_is_read_exactly() {
    let s = claude_code::parse(&read("minimal.jsonl"));
    assert_eq!(s.turns.len(), 1, "one assistant turn expected");
    assert_eq!(s.skipped, 0);
    let seen: Vec<_> = s
        .turns
        .iter()
        .map(|t| (t.input, t.cache_creation, t.cache_read, t.output))
        .collect();
    assert_eq!(seen, [(Some(11), Some(100), Some(1000), Some(22))]);
    // Billed input is fresh + creation + read -- cache reads ARE billed,
    // which is the whole reason the runtime axis exists.
    assert_eq!(s.billed_input(), 1111);
}

/// V47: absent usage stays absent. A zero would read as a measurement.
#[test]
fn missing_usage_is_none_not_zero() {
    let s = claude_code::parse(&read("weird.jsonl"));
    let unaccounted: Vec<_> = s
        .turns
        .iter()
        .filter(|t| t.billed_input().is_none())
        .map(|t| (t.input, t.cache_read))
        .collect();
    assert_eq!(
        unaccounted,
        [(None, None)],
        "the record with no usage stays a TURN, with absent fields absent"
    );
}

/// What `tool-shapes.jsonl` must yield: one event per result, in file
/// order, each labelled by the SHAPE observed and carrying the path when
/// the tool named one.
const TOOL_SHAPES: [(&str, Option<&str>); 4] = [
    ("shell", None),
    ("read", Some("/w/one.txt")),
    ("edit", Some("/w/one.txt")),
    ("write", Some("/w/two.txt")),
];

/// V76's corollary: result shapes are tool-specific. Each must yield an
/// event, and the ones naming a file must carry the path.
#[test]
fn every_tool_shape_yields_an_event() {
    let s = claude_code::parse(&read("tool-shapes.jsonl"));
    assert_eq!(s.skipped, 0);
    let seen: Vec<(&str, Option<&str>)> = s
        .events
        .iter()
        .map(|e| (e.source.label(), e.path.as_deref()))
        .collect();
    assert_eq!(seen, TOOL_SHAPES);
}

/// V76, the one that matters: the transcript records what was BILLED. The
/// spilled size is reported separately and must NEVER become the load
/// size -- that direction overcounts by orders of magnitude.
#[test]
fn billed_size_is_the_retained_content_not_the_spill() {
    let s = claude_code::parse(&read("truncated.jsonl"));
    let seen: Vec<_> = s.events.iter().map(|e| (e.bytes, e.spilled)).collect();
    assert_eq!(
        seen,
        [(40, Some(4_000_000))],
        "billed = retained stdout; the spill is reported, never billed"
    );
    assert_eq!(
        s.accounted_bytes(),
        40,
        "the 4MB spill must not reach the accounted total"
    );
}

/// V43: the file appends live, so the last line may be half-written. A
/// torn tail is normal -- the whole records before it still parse, and
/// the tear is COUNTED rather than silently dropped.
#[test]
fn a_torn_tail_is_tolerated_and_counted() {
    let s = claude_code::parse(&read("torn-tail.jsonl"));
    assert_eq!(s.turns.len(), 1, "the complete record still parses");
    assert_eq!(s.skipped, 1, "the torn line is counted, not ignored");
}

/// V43: a malformed record is skipped and counted, never fatal. The
/// garbage sits mid-file with a good record after it, so a reader that
/// aborted on first error fails here instead of quietly reading less.
#[test]
fn garbage_is_skipped_and_parsing_continues() {
    let s = claude_code::parse(&read("weird.jsonl"));
    assert_eq!(s.skipped, 1, "exactly the one garbage line");
    let after = s.turns.iter().any(|t| t.billed_input() == Some(99));
    assert!(
        after,
        "the record AFTER the garbage line must still be read"
    );
}

/// An unrecognised result shape still yields an event, labelled by what
/// it IS rather than by a guessed tool name (V3). And a blank line is
/// skipped WITHOUT counting as malformed: absence is not damage.
#[test]
fn an_unknown_result_shape_is_labelled_not_guessed() {
    let s = claude_code::parse(&read("weird.jsonl"));
    let generic: Vec<usize> = s
        .events
        .iter()
        .filter(|e| e.source.label() == "tool")
        .map(|e| e.bytes)
        .collect();
    assert_eq!(generic, [0], "unknown shape: no content found, size 0");
    assert_eq!(s.skipped, 1, "a blank line is not a malformed record");
}

/// V78: an attachment payload may nest arrays and non-string leaves. Only
/// string content counts -- a number never entered the context as text.
#[test]
fn attachment_payloads_count_only_string_content() {
    let s = claude_code::parse(&read("weird.jsonl"));
    let queued: Vec<usize> = s
        .events
        .iter()
        .filter(|e| e.source.label() == "queued_command")
        .map(|e| e.bytes)
        .collect();
    let want = "abc".len().saturating_add("de".len());
    assert_eq!(queued, [want], "strings only");
}

/// V43: unknown record types and unknown fields are ignored, not errors.
/// A harness adds both without warning.
#[test]
fn unknown_records_and_fields_are_ignored() {
    let s = claude_code::parse(&read("weird.jsonl"));
    // The unknown "some-future-record-type" contributed nothing and broke
    // nothing; the records around it were still read.
    assert!(!s.turns.is_empty(), "known records still parse");
    assert!(
        s.turns.iter().any(|t| t.input == Some(7)),
        "a record carrying an unknown usage field is still read"
    );
}

/// V76: a result that is a bare STRING rather than an object still yields
/// an event sized by its content.
#[test]
fn a_bare_string_result_is_handled() {
    let s = claude_code::parse(&read("weird.jsonl"));
    let sizes: Vec<usize> = s
        .events
        .iter()
        .filter(|e| e.source.label() == "text")
        .map(|e| e.bytes)
        .collect();
    assert_eq!(sizes, ["a bare string, not an object".len()]);
}

/// Label and billed size for each attachment in `weird.jsonl`.
fn expected_attachments() -> Vec<(String, usize)> {
    let hook = "SessionStart".len().saturating_add("0123456789".len());
    let queued = "abc".len().saturating_add("de".len());
    vec![
        ("hook_success".to_owned(), hook),
        ("queued_command".to_owned(), queued),
    ]
}

/// V78: attachments are a load class. Hook output costs input tokens like
/// anything else, and counting only tool results would inflate the
/// unaccounted gap while the data sat right there.
#[test]
fn attachments_are_counted_as_loads() {
    let s = claude_code::parse(&read("weird.jsonl"));
    let seen: Vec<_> = s
        .events
        .iter()
        .filter(|e| matches!(e.source, Source::Attachment(_)))
        .map(|e| (e.source.label().to_owned(), e.bytes))
        .collect();
    assert_eq!(seen, expected_attachments());
}

/// V43: the reader is READ-ONLY. The transcript is the user's data and
/// the ground truth; itok never writes, moves or mutates it.
#[test]
fn parsing_never_touches_the_file() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/session/weird.jsonl");
    let before = std::fs::read(&p).unwrap_or_default();
    let mtime = std::fs::metadata(&p).ok().and_then(|m| m.modified().ok());
    let _ = claude_code::parse(&read("weird.jsonl"));
    assert_eq!(
        std::fs::read(&p).unwrap_or_default(),
        before,
        "bytes changed"
    );
    assert_eq!(
        std::fs::metadata(&p).ok().and_then(|m| m.modified().ok()),
        mtime,
        "mtime changed -- something wrote to the transcript"
    );
}

/// Garbage in, empty out -- never a panic. "This file is not a
/// transcript" is a legitimate answer, and the skip count says so.
#[test]
fn wholly_unparsable_input_yields_a_counted_empty_session() {
    let s = claude_code::parse("not json\nalso not json\n");
    assert!(s.turns.is_empty() && s.events.is_empty());
    assert_eq!(s.skipped, 2);
    assert_eq!(s.billed_input(), 0);
}

/// V77: the prefix ends at a record boundary, so a half-written tail is
/// simply not there.
#[test]
fn the_complete_prefix_excludes_a_torn_tail() {
    let torn = read("torn-tail.jsonl");
    let prefix = itok::session::complete_prefix(&torn);
    assert!(prefix.len() < torn.len(), "the torn tail must be dropped");
    assert!(prefix.ends_with('\n'), "the prefix ends at a boundary");
    // And parsing the prefix finds nothing to skip: the tear was absence,
    // not damage. Parsing the RAW text still counts it, because `parse`
    // reads exactly what it is given.
    assert_eq!(claude_code::parse(prefix).skipped, 0);
    assert_eq!(claude_code::parse(&torn).skipped, 1);
}

/// V77: appending a PARTIAL record leaves the key unchanged -- that is
/// the whole point. Two reads a second apart agree while the harness is
/// mid-write.
#[test]
fn a_partial_append_does_not_change_the_key() {
    let base = read("minimal.jsonl");
    let mid_write = format!("{base}{{\"type\":\"assist");
    assert_eq!(
        itok::session::content_key(&base),
        itok::session::content_key(&mid_write),
        "a half-written record must not change the answer"
    );
}

/// A WHOLE new record does change it -- otherwise the key would not
/// track session state at all.
#[test]
fn a_complete_append_changes_the_key() {
    let base = read("minimal.jsonl");
    let grown = format!("{base}{{\"type\":\"user\",\"uuid\":\"u9\"}}\n");
    assert_ne!(
        itok::session::content_key(&base),
        itok::session::content_key(&grown)
    );
}

/// Deterministic: the same content always yields the same key, in this
/// process and any other. That is why the hash is hand-rolled rather than
/// `DefaultHasher`, which is not stable across Rust releases.
#[test]
fn the_key_is_deterministic_and_content_addressed() {
    let a = read("weird.jsonl");
    assert_eq!(
        itok::session::content_key(&a),
        itok::session::content_key(&a)
    );
    // Same length, different bytes -> different key.
    assert_ne!(
        itok::session::content_key("aaaa\n"),
        itok::session::content_key("aaab\n")
    );
    // The key names the prefix length, so a reader can see what it covers.
    assert!(itok::session::content_key("aaaa\n").starts_with("5-"));
}

/// Nothing complete yet: no newline means no whole record.
#[test]
fn a_prefix_with_no_newline_is_empty() {
    assert_eq!(itok::session::complete_prefix("{\"type\":\"assi"), "");
    assert_eq!(itok::session::complete_prefix(""), "");
    assert_eq!(
        claude_code::parse(itok::session::complete_prefix("x")).skipped,
        0
    );
}
