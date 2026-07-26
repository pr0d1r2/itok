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

use super::{LoadEvent, Session, Source, Turn};
use serde_json::Value;

/// Parse a whole transcript. Never fails: unreadable input yields an
/// empty session with a skip count, because "this file is garbage" and
/// "this session did nothing" are both legitimate answers here.
#[must_use]
pub fn parse(text: &str) -> Session {
    let mut out = Session::default();
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
            Ok(v) => absorb(&v, &mut out),
        }
    }
    out
}

/// Route one record. An unknown `type` is ignored, not an error: the
/// harness adds record kinds without warning, and a reader that rejected
/// them would break on every upgrade.
fn absorb(v: &Value, out: &mut Session) {
    match str_at(v, "type") {
        Some("assistant") => out.turns.extend(turn(v)),
        Some("user") => out.events.extend(tool_event(v)),
        Some("attachment") => out.events.extend(attachment_event(v)),
        _ => {}
    }
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
fn turn(v: &Value) -> Option<Turn> {
    let usage = usage_of(v);
    let num = |k: &str| usage.get(k).and_then(Value::as_u64);
    Some(Turn {
        session: session_of(v),
        ts: ts_of(v),
        input: num("input_tokens"),
        cache_creation: num("cache_creation_input_tokens"),
        cache_read: num("cache_read_input_tokens"),
        output: num("output_tokens"),
    })
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
