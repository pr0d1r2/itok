//! The Claude Code transcript shape: JSONL under `~/.claude/projects/`,
//! one record per line.
//!
//! Everything here was derived by CHARACTERISING a real transcript rather
//! than from documentation, because none exists and the schema is not
//! versioned. The shapes handled below are the ones actually observed:
//! per-tool result objects, results that are a bare string, records with
//! no `usage`, unknown record types, unknown fields, a null `isSidechain`,
//! a garbage line mid-file, and a half-written final line.
//!
//! Defensive by construction (V43): every accessor is an `Option` chain,
//! nothing panics, and an unreadable record increments `skipped` instead
//! of aborting the parse. A reader that stopped at the first bad line
//! would silently report less rather than fail, which is worse.

use super::{Compaction, LoadEvent, Session, Source, Turn};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Parse a whole transcript. Never fails: unreadable input yields an
/// empty session with a skip count, because "this file is garbage" and
/// "this session did nothing" are both legitimate answers here.
#[must_use]
pub fn parse(text: &str) -> Session {
    parse_with(text).0
}

/// The same pass, keeping the compaction boundaries beside the session.
///
/// They travel SEPARATELY rather than inside `Session` because that type
/// is public and constructible by literal: a new field on it breaks every
/// downstream construction, and the badge's red line is not worth a
/// version rung (V39). The parse itself is one pass either way.
#[must_use]
pub fn parse_with(text: &str) -> (Session, Vec<Compaction>) {
    let mut out = Session::default();
    let mut compactions = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(line) {
            // A torn tail is NORMAL, not an error: the file appends while
            // the session runs, so the last line may be half-written
            // (V43). It still counts as skipped -- the number is the
            // honest signal that something was not read.
            Err(_) => out.skipped = out.skipped.saturating_add(1),
            Ok(v) => absorb(&v, &mut out, &mut compactions),
        }
    }
    (out, compactions)
}

/// Route one record. An unknown `type` is ignored, not an error: the
/// harness adds record kinds without warning, and a reader that rejected
/// them would break on every upgrade.
fn absorb(v: &Value, out: &mut Session, compactions: &mut Vec<Compaction>) {
    // Every record contributes to the CONTEXT, not just the ones that
    // become load events: the conversation itself is re-sent each turn and
    // is the bulk of a long window (V102). Counted as byte LENGTHS only --
    // no text enters any type here (V45).
    out.content_bytes = out.content_bytes.saturating_add(content_bytes(v));
    match str_at(v, "type") {
        Some("assistant") => out.turns.extend(turn(v, out.content_bytes)),
        Some("user") => out.events.extend(tool_event(v)),
        Some("attachment") => out.events.extend(attachment_event(v)),
        Some("system") => compactions.extend(compaction(v)),
        _ => {}
    }
}

/// One compaction boundary. The record is `type:"system"` with
/// `subtype:"compact_boundary"`, and the numbers under `compactMetadata`
/// are the HARNESS's own, which is what makes the auto-compact point a
/// measurement rather than a guess (V116).
///
/// Keyed off the metadata OBJECT, not the subtype string: the subtype is
/// a label the harness may rename, while a record carrying
/// `compactMetadata` is a compaction whatever it calls itself. A record
/// without `preTokens` is skipped rather than defaulted -- an absent
/// trigger point read as 0 would draw the red line at the origin and
/// paint every session red (V47).
fn compaction(v: &Value) -> Option<Compaction> {
    let meta = v.get("compactMetadata")?;
    Some(Compaction {
        trigger: str_at(meta, "trigger").unwrap_or_default().to_owned(),
        pre_tokens: meta.get("preTokens")?.as_u64()?,
        post_tokens: meta
            .get("postTokens")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        ts: ts_of(v),
    })
}

fn str_at<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key)?.as_str()
}

fn session_of(v: &Value) -> String {
    str_at(v, "sessionId").unwrap_or_default().to_owned()
}

fn ts_of(v: &Value) -> String {
    str_at(v, "timestamp").unwrap_or_default().to_owned()
}

/// One turn's usage. Absent fields stay `None` -- a zero would read as a
/// measurement (V47).
///
/// EVERY assistant record becomes a turn, including one carrying no
/// `usage` at all. Dropping it would be the V44 failure: the token total
/// would still look exact while the turn COUNT quietly under-reported,
/// hiding precisely the gap that "accounted vs unaccounted" exists to
/// show. On the transcript characterised, usage was present on 495 of
/// 495 records -- but that is an observation about one harness version,
/// not a guarantee, and the honest shape does not depend on it holding.
fn turn(v: &Value, cum_content_bytes: u64) -> Option<Turn> {
    let usage = usage_of(v);
    let num = |k: &str| usage.get(k).and_then(Value::as_u64);
    Some(Turn {
        session: session_of(v),
        ts: ts_of(v),
        input: num("input_tokens"),
        cache_creation: num("cache_creation_input_tokens"),
        cache_read: num("cache_read_input_tokens"),
        output: num("output_tokens"),
        cum_content_bytes,
    })
}

/// Byte length of everything this record puts into the context.
///
/// Sums the content blocks a request carries back: visible `text`,
/// `tool_use` ARGUMENTS (a Write's body is an input token like any other),
/// `tool_result` payloads, and a `thinking` block's SIGNATURE.
///
/// The signature, not the reasoning: measured, `thinking` is stored EMPTY
/// with an ~808-byte signature beside it, so the reasoning text is not
/// recoverable here and whatever it costs lands in the fit's slope instead
/// (V102 says so rather than pretending the slope is a tokenizer ratio).
///
/// Lengths only -- this returns a count and keeps no text (V45).
fn content_bytes(v: &Value) -> u64 {
    let Some(c) = v.get("message").and_then(|m| m.get("content")) else {
        return 0;
    };
    match c {
        Value::String(s) => len_of(s.len()),
        Value::Array(blocks) => blocks.iter().map(block_bytes).sum(),
        _ => 0,
    }
}

/// One content block's byte length, by its own shape.
fn block_bytes(b: &Value) -> u64 {
    let field = |k: &str| b.get(k).and_then(Value::as_str).map_or(0, str::len);
    let json = |k: &str| b.get(k).map_or(0, |x| x.to_string().len());
    len_of(match str_at(b, "type") {
        Some("text") => field("text"),
        // Empty in practice; the signature is what actually travels.
        Some("thinking") => {
            field("signature").saturating_add(field("thinking"))
        }
        Some("tool_use") => json("input"),
        Some("tool_result") => json("content"),
        _ => 0,
    })
}

fn len_of(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

/// The `usage` object, or `Null` when the record carries none.
fn usage_of(v: &Value) -> Value {
    v.get("message")
        .and_then(|m| m.get("usage"))
        .cloned()
        .unwrap_or(Value::Null)
}

/// A tool result carried on a user record.
///
/// The shape is TOOL-SPECIFIC and sometimes a bare string rather than an
/// object (V76), so this reads what is there and never assumes a schema.
fn tool_event(v: &Value) -> Option<LoadEvent> {
    let result = v.get("toolUseResult")?;
    let (bytes, spilled, path) = measure(result);
    Some(LoadEvent {
        session: session_of(v),
        ts: ts_of(v),
        source: Source::Tool(tool_name(result).to_owned()),
        path,
        bytes,
        spilled,
    })
}

/// `(billed, spilled, path)` for one result, whatever its shape.
fn measure(result: &Value) -> (usize, Option<usize>, Option<String>) {
    match result {
        Value::String(s) => (s.len(), None, None),
        _ => (
            retained_bytes(result),
            spilled_bytes(result),
            path_of(result),
        ),
    }
}

/// Bytes that actually entered the context.
///
/// V76: this is the RETAINED content, never `persistedOutputSize`. A
/// harness truncates an oversized result and spills the rest to a side
/// file; billing the on-disk size would overcount by orders of magnitude.
fn retained_bytes(result: &Value) -> usize {
    let field =
        |k: &str| result.get(k).and_then(Value::as_str).map_or(0, str::len);
    let streams = field("stdout")
        .saturating_add(field("stderr"))
        .saturating_add(field("content"));
    if streams > 0 {
        return streams;
    }
    // A Read result nests the content one level down.
    result
        .get("file")
        .and_then(|f| f.get("content"))
        .and_then(Value::as_str)
        .map_or(0, str::len)
}

/// Bytes spilled to a side file: reported, never billed (V76).
fn spilled_bytes(result: &Value) -> Option<usize> {
    let n = result.get("persistedOutputSize")?.as_u64()?;
    usize::try_from(n).ok()
}

fn path_of(result: &Value) -> Option<String> {
    let direct = result.get("filePath").and_then(Value::as_str);
    let nested = result
        .get("file")
        .and_then(|f| f.get("filePath"))
        .and_then(Value::as_str);
    direct.or(nested).map(str::to_owned)
}

/// A label for the result's shape. The transcript does not name the tool
/// on the result record, so this reports the SHAPE observed rather than
/// guessing a tool name -- an invented name would be a confident lie (V3).
fn tool_name(result: &Value) -> &'static str {
    if result.is_string() {
        return "text";
    }
    by_key(result).unwrap_or_else(|| by_type(result))
}

/// A distinguishing key present on the result.
fn by_key(result: &Value) -> Option<&'static str> {
    BY_KEY
        .iter()
        .find(|(key, _)| result.get(key).is_some())
        .map(|(_, label)| *label)
}

/// The result's own `type` tag, when it carries one.
fn by_type(result: &Value) -> &'static str {
    match str_at(result, "type") {
        Some("create" | "update") => "write",
        Some("text") => "read",
        _ => "tool",
    }
}

/// A distinguishing key -> the shape it identifies.
const BY_KEY: [(&str, &str); 3] = [
    ("stdout", "shell"),
    ("oldString", "edit"),
    ("persistedOutputPath", "shell"),
];

/// Hook output and injected context (V78).
fn attachment_event(v: &Value) -> Option<LoadEvent> {
    let a = v.get("attachment")?;
    let kind = str_at(a, "type").unwrap_or("attachment").to_owned();
    Some(LoadEvent {
        session: session_of(v),
        ts: ts_of(v),
        source: Source::Attachment(kind),
        path: None,
        bytes: attachment_bytes(a),
        spilled: None,
    })
}

/// An attachment's payload size. Only string leaves are counted -- the
/// structure itself is not content that was billed.
fn attachment_bytes(a: &Value) -> usize {
    match a {
        Value::String(s) => s.len(),
        Value::Object(map) => map
            .iter()
            .filter(|(k, _)| k.as_str() != "type")
            .map(|(_, v)| attachment_bytes(v))
            .fold(0usize, usize::saturating_add),
        Value::Array(items) => items
            .iter()
            .map(attachment_bytes)
            .fold(0usize, usize::saturating_add),
        _ => 0,
    }
}

/// The directory this harness keeps a project's transcripts in.
///
/// Claude Code encodes the project path by replacing every separator with
/// `-`, e.g. `/w/proj` -> `-w-proj`. Derived rather than hardcoded, so it
/// follows the caller's cwd (V37).
#[must_use]
pub fn project_dir(home: &Path, project: &Path) -> PathBuf {
    let encoded: String = project
        .to_string_lossy()
        .chars()
        .map(|c| if c == '/' || c == '\\' { '-' } else { c })
        .collect();
    home.join(".claude").join("projects").join(encoded)
}

/// The newest transcript for a project, or `None` when there is none.
///
/// "Newest" is by modification time: a session that is still running is
/// the one being appended to, which is almost always what a bare `itok
/// trace` means (V1 -- `git log` defaults to the current repo).
#[must_use]
pub fn newest_transcript(home: &Path, project: &Path) -> Option<PathBuf> {
    newest_in(&project_dir(home, project))
}

/// The newest `.jsonl` in a transcript directory. Split out so a caller can
/// reach it after resolving the directory by another route (T74).
#[must_use]
pub fn newest_in(dir: &Path) -> Option<PathBuf> {
    let dir = dir.to_path_buf();
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|x| x != "jsonl") {
            continue;
        }
        let Ok(when) = entry.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        if best.as_ref().is_none_or(|(b, _)| when > *b) {
            best = Some((when, path));
        }
    }
    best.map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Per-PID, so the suite stays parallel-safe (V89/B5).
    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir()
            .join(format!("itok-cc-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        d
    }

    /// The harness encodes a project path by replacing separators.
    /// The harness's own record of where auto-compact fired (V116).
    /// Parsed off `compactMetadata` rather than off the subtype label,
    /// which is the harness's to rename.
    #[test]
    fn a_compact_boundary_becomes_a_compaction() {
        let line = r#"{"type":"system","subtype":"compact_boundary","timestamp":"2026-09-05T12:29:54.086Z","compactMetadata":{"trigger":"auto","preTokens":968887,"postTokens":11503,"cumulativeDroppedTokens":957384}}"#;
        let (_, got) = super::parse_with(line);
        assert_eq!(got.len(), 1);
        assert_eq!(super::super::compact_point(&got), Some(968_887));
        assert_eq!(super::super::auto_compactions(&got).count(), 1);
    }

    /// V116: a human compacting early says what that human wanted, not
    /// where the machine would have fired. Excluded from the point, and
    /// still RECORDED, because it did happen to this context.
    #[test]
    fn a_manual_compaction_is_recorded_but_never_sets_the_point() {
        let line = r#"{"type":"system","subtype":"compact_boundary","compactMetadata":{"trigger":"manual","preTokens":120000,"postTokens":9000}}"#;
        let (_, got) = super::parse_with(line);
        assert_eq!(got.len(), 1);
        assert_eq!(super::super::compact_point(&got), None);
    }

    /// The lowest auto point wins: it is the one that fired earliest, so
    /// a line drawn there warns before every point actually seen.
    #[test]
    fn the_lowest_auto_point_is_the_one_reported() {
        let text = concat!(
            r#"{"type":"system","compactMetadata":{"trigger":"auto","preTokens":1000418,"postTokens":1}}"#,
            "\n",
            r#"{"type":"system","compactMetadata":{"trigger":"auto","preTokens":968887,"postTokens":1}}"#,
        );
        let (_, got) = super::parse_with(text);
        assert_eq!(super::super::compact_point(&got), Some(968_887));
        assert_eq!(super::super::auto_compactions(&got).count(), 2);
    }

    /// A boundary with no `preTokens` is skipped, not defaulted: a zero
    /// trigger point would draw the red line at the origin (V47).
    #[test]
    fn a_boundary_without_a_pre_count_is_not_a_zero() {
        let line = r#"{"type":"system","compactMetadata":{"trigger":"auto"}}"#;
        let (_, got) = super::parse_with(line);
        assert!(got.is_empty());
        assert_eq!(super::super::compact_point(&got), None);
    }

    #[test]
    fn project_dir_encodes_the_path() {
        let got = project_dir(Path::new("/home/u"), Path::new("/w/proj"));
        assert_eq!(got, Path::new("/home/u/.claude/projects/-w-proj"));
    }

    /// No directory is not an error -- the project simply has no sessions.
    #[test]
    fn no_project_dir_yields_none() {
        let home = scratch("nodir");
        assert!(newest_transcript(&home, Path::new("/w/proj")).is_none());
    }

    /// Only transcripts count; a stray file in the same directory is not
    /// one just because it is there.
    #[test]
    fn only_jsonl_files_are_transcripts() {
        let home = scratch("filter");
        let dir = project_dir(&home, Path::new("/w/p"));
        std::fs::create_dir_all(&dir).ok();
        std::fs::write(dir.join("notes.txt"), "x").ok();
        std::fs::write(dir.join("a.jsonl"), "{}\n").ok();
        let got = newest_transcript(&home, Path::new("/w/p"));
        assert_eq!(got, Some(dir.join("a.jsonl")));
    }

    /// A directory with no transcript is the same as no directory.
    #[test]
    fn a_dir_without_transcripts_yields_none() {
        let home = scratch("empty");
        let dir = project_dir(&home, Path::new("/w/p"));
        std::fs::create_dir_all(&dir).ok();
        std::fs::write(dir.join("readme.md"), "x").ok();
        assert!(newest_transcript(&home, Path::new("/w/p")).is_none());
    }

    /// With several transcripts, one is chosen -- and it is always one of
    /// them. Which one depends on mtime, which this test deliberately does
    /// not race: asserting an ordering would need controllable clocks, and
    /// a sleep in a test buys flakiness rather than confidence (B5).
    #[test]
    fn several_transcripts_yield_one_of_them() {
        let home = scratch("many");
        let dir = project_dir(&home, Path::new("/w/p"));
        std::fs::create_dir_all(&dir).ok();
        for name in ["a.jsonl", "b.jsonl"] {
            std::fs::write(dir.join(name), "{}\n").ok();
        }
        let got = newest_transcript(&home, Path::new("/w/p"));
        let ok = got == Some(dir.join("a.jsonl"))
            || got == Some(dir.join("b.jsonl"));
        assert!(ok, "picked {got:?}");
    }
}
