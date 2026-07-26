//! The `trace` command: one line per runtime load event, chronological
//! (V46). `git log`/strace-shaped -- `-n`, `--since`, `--reverse` mean
//! what they mean there, so there is nothing new to learn (V1).
//!
//! Report-only, exit 0 always (V5). It reports ARITHMETIC -- what was
//! loaded, when, and how big -- never a verdict about whether that is
//! healthy (V59).
//!
//! Per-event sizes are ESTIMATES and say so. The reader keeps no content
//! (V45), only byte lengths, so `bytes/4` is the only tier available
//! here: there is nothing left to tokenize, which is why `--bpe` is not
//! accepted rather than accepted-and-ignored (V3). The session TOTAL is
//! exact, because it comes from the harness's own usage records -- the
//! attribution is partial, the total is not (V44).

use crate::args::Format;
use crate::cli::Output;
use crate::json::escape;
use crate::session::{claude_code, LoadEvent, Source};
use std::path::PathBuf;

#[derive(Default)]
struct Raw {
    session: Option<String>,
    limit: Option<String>,
    since: Option<String>,
    reverse: bool,
    format: Format,
    chdir: Option<String>,
}

pub(crate) fn trace(rest: &[String]) -> Output {
    match parse(rest) {
        Ok(raw) => run(&raw),
        Err(e) => Output::usage_err(format!("itok: {e}")),
    }
}

fn run(raw: &Raw) -> Output {
    let Some(path) = source_path(raw) else {
        // No transcript is not an error: this project may simply have no
        // recorded session. Report-only means exit 0 (V5).
        return Output::ok(String::new());
    };
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    // Truncate at the last complete line so a session still being written
    // gives the same answer twice (V77/V43).
    let parsed = claude_code::parse(crate::session::complete_prefix(&text));
    let rows = select(parsed.events, raw);
    Output::ok(match raw.format {
        Format::Json => json(&rows),
        Format::Human => report(&rows),
    })
}

/// An explicit path wins; otherwise the newest transcript for the working
/// directory, the way `git log` defaults to the current repo (V1).
fn source_path(raw: &Raw) -> Option<PathBuf> {
    if let Some(s) = &raw.session {
        return Some(PathBuf::from(s));
    }
    let cwd = raw.chdir.as_ref().map_or_else(
        || std::env::current_dir().ok(),
        |d| Some(PathBuf::from(d)),
    )?;
    let home = std::env::var("HOME").ok().map(PathBuf::from)?;
    let project = std::fs::canonicalize(&cwd).unwrap_or(cwd);
    claude_code::newest_transcript(&home, &project)
}

/// Apply the git-log selectors, in git's own order: filter, then reverse,
/// then limit. `-n` therefore keeps the NEWEST events, like `git log -n`
/// keeps the most recent commits (V1).
fn select(events: Vec<LoadEvent>, raw: &Raw) -> Vec<LoadEvent> {
    let mut rows = since(events, raw.since.as_deref());
    keep_newest(&mut rows, raw.limit.as_deref());
    if raw.reverse {
        rows.reverse();
    }
    rows
}

/// ISO-8601 sorts lexicographically, which is why a plain string
/// comparison is correct here and no date library is needed.
fn since(events: Vec<LoadEvent>, cutoff: Option<&str>) -> Vec<LoadEvent> {
    events
        .into_iter()
        .filter(|e| cutoff.is_none_or(|s| e.ts.as_str() >= s))
        .collect()
}

/// `-n` keeps the NEWEST events, like `git log -n` keeps the most recent
/// commits (V1). An unparsable count keeps everything, rather than
/// silently keeping none.
fn keep_newest(rows: &mut Vec<LoadEvent>, limit: Option<&str>) {
    let Some(n) = limit.and_then(|n| n.parse::<usize>().ok()) else {
        return;
    };
    let drop = rows.len().saturating_sub(n);
    rows.drain(..drop);
}

/// `bytes/4`, the only tier available without content (V45). The tilde in
/// the human view marks it crude, exactly as everywhere else (V3).
fn tokens(bytes: usize) -> usize {
    bytes / 4
}

fn kind(source: &Source) -> &'static str {
    match source {
        Source::Tool(_) => "tool",
        Source::Attachment(_) => "attachment",
    }
}

/// One line per event: time, what it came from, its estimated cost, and
/// the path when there is one.
fn report(rows: &[LoadEvent]) -> String {
    let mut out = String::new();
    for e in rows {
        out.push_str(&format!(
            "{}  {:<22} ~{:>7} itok  {}\n",
            time_of(&e.ts),
            e.source.label(),
            tokens(e.bytes),
            e.path.as_deref().unwrap_or(""),
        ));
    }
    out
}

/// The clock part of an ISO timestamp, which is what a chronological
/// listing needs; the date is the same for every line in a session.
fn time_of(ts: &str) -> &str {
    ts.split('T')
        .nth(1)
        .and_then(|t| t.get(..8))
        .unwrap_or("--:--:--")
}

/// One JSON object per event (V9). Carries the same `unit`/`estimated`/
/// `method` fields every other verb emits, so a parser learns them once.
fn json(rows: &[LoadEvent]) -> String {
    rows.iter().map(json_object).collect()
}

fn json_object(e: &LoadEvent) -> String {
    format!(
        "{{\"ts\":\"{}\",\"kind\":\"{}\",\"source\":\"{}\",\"path\":{},\
         \"bytes\":{},\"tokens\":{},\"unit\":\"input_tokens\",\
         \"estimated\":true,\"method\":\"bytes/4\"}}\n",
        escape(&e.ts),
        kind(&e.source),
        escape(e.source.label()),
        json_path(e.path.as_deref()),
        e.bytes,
        tokens(e.bytes),
    )
}

/// A quoted path, or the `null` literal -- never an empty string, which
/// would be indistinguishable from a path the tool failed to read.
fn json_path(path: Option<&str>) -> String {
    path.map_or_else(|| "null".to_owned(), |p| format!("\"{}\"", escape(p)))
}

fn parse(rest: &[String]) -> Result<Raw, String> {
    let mut raw = Raw::default();
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        apply(a, &mut it, &mut raw)?;
    }
    Ok(raw)
}

/// One argument. Flags mirror `git log`'s, so there is nothing new to
/// learn (V1); a bare word is the session to read.
fn apply<'a>(
    a: &str,
    it: &mut impl Iterator<Item = &'a String>,
    raw: &mut Raw,
) -> Result<(), String> {
    match a {
        "--reverse" => raw.reverse = true,
        "-n" => raw.limit = Some(value(it, "-n")?),
        "--since" => raw.since = Some(value(it, "--since")?),
        "-C" => raw.chdir = Some(value(it, "-C")?),
        "--format" => raw.format = format_of(&value(it, "--format")?)?,
        "--bpe" | "--ollama" => return Err(no_real_tier(a)),
        other if other.starts_with('-') => {
            return Err(format!("unknown flag {other}"))
        }
        other => raw.session = Some(other.to_owned()),
    }
    Ok(())
}

/// Rejected, not ignored: no content is retained, so there is nothing
/// for a real tokenizer to count (V3/V45). Accepting the flag and
/// quietly producing an estimate would be the confident lie V3 forbids.
fn no_real_tier(flag: &str) -> String {
    format!(
        "{flag} is not available on trace: no content is stored, \
         so per-event sizes are always an estimate"
    )
}

/// The two documented output shapes; anything else is a usage error, the
/// same wording every other verb uses (V9).
fn format_of(s: &str) -> Result<Format, String> {
    match s {
        "json" => Ok(Format::Json),
        "human" => Ok(Format::Human),
        other => Err(format!("unknown format '{other}'")),
    }
}

fn value<'a>(
    it: &mut impl Iterator<Item = &'a String>,
    flag: &str,
) -> Result<String, String> {
    it.next()
        .cloned()
        .ok_or_else(|| format!("{flag} needs a value"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        format!(
            "{}/tests/fixtures/session/{name}",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    fn run_on(name: &str, extra: &[&str]) -> Output {
        let mut args = vec![fixture(name)];
        args.extend(extra.iter().map(|s| (*s).to_owned()));
        trace(&args)
    }

    /// V46: one line per load event.
    #[test]
    fn one_line_per_event() {
        let out = run_on("tool-shapes.jsonl", &[]);
        assert_eq!(out.out.lines().count(), 4);
        assert_eq!(out.code, 0);
    }

    /// V46/V1: `-n` keeps the NEWEST events, like `git log -n`.
    #[test]
    fn limit_keeps_the_newest() {
        let out = run_on("tool-shapes.jsonl", &["-n", "2"]);
        let lines: Vec<&str> = out.out.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().any(|l| l.contains("two.txt")), "kept newest");
    }

    /// V46: `--reverse` flips the order, exactly as in git.
    #[test]
    fn reverse_flips_the_order() {
        let fwd = run_on("tool-shapes.jsonl", &[]);
        let rev = run_on("tool-shapes.jsonl", &["--reverse"]);
        let f: Vec<&str> = fwd.out.lines().collect();
        let mut r: Vec<&str> = rev.out.lines().collect();
        r.reverse();
        assert_eq!(f, r);
    }

    /// `--since` filters on the ISO timestamp. Lexicographic comparison is
    /// correct for ISO-8601, which is why no date library is needed.
    #[test]
    fn since_filters_by_iso_timestamp() {
        let all = run_on("tool-shapes.jsonl", &[]);
        let cut =
            run_on("tool-shapes.jsonl", &["--since", "2026-01-01T00:00:05"]);
        assert!(cut.out.lines().count() < all.out.lines().count());
        assert!(cut.out.contains("two.txt"), "later events survive");
    }

    /// V9: one JSON object per event, carrying the same unit/estimated/
    /// method fields every other verb emits.
    #[test]
    fn json_is_one_object_per_event() {
        let out = run_on("tool-shapes.jsonl", &["--format", "json"]);
        assert_eq!(out.out.lines().count(), 4);
        for line in out.out.lines() {
            assert!(line.starts_with('{') && line.ends_with('}'));
            assert!(line.contains("\"unit\":\"input_tokens\""));
            assert!(line.contains("\"estimated\":true"));
            assert!(line.contains("\"method\":\"bytes/4\""));
        }
    }

    /// V3: no tilde ever appears in a numeric JSON field; the human view
    /// carries it instead.
    #[test]
    fn the_tilde_is_human_only() {
        assert!(run_on("tool-shapes.jsonl", &[]).out.contains('~'));
        assert!(!run_on("tool-shapes.jsonl", &["--format", "json"])
            .out
            .contains('~'));
    }

    /// V3/V45: a real tokenizer is REJECTED rather than accepted and
    /// ignored -- no content is stored, so there is nothing to count.
    #[test]
    fn a_real_tier_is_refused_not_ignored() {
        for flag in ["--bpe", "--ollama"] {
            let out = run_on("tool-shapes.jsonl", &[flag]);
            assert_eq!(out.code, 2, "{flag} must be a usage error");
            assert!(out.err.contains("no content is stored"));
        }
    }

    /// V5: report-only. A missing or unreadable transcript is not an
    /// error -- it means there is nothing to report.
    #[test]
    fn a_missing_transcript_is_empty_and_exit_zero() {
        let out = trace(&["/nonexistent/session.jsonl".to_owned()]);
        assert_eq!(out.code, 0);
        assert!(out.out.is_empty());
    }

    /// V43: a torn tail must not change the answer, so a session still
    /// being written reports the same thing twice.
    #[test]
    fn a_torn_tail_does_not_break_the_listing() {
        let out = run_on("torn-tail.jsonl", &[]);
        assert_eq!(out.code, 0);
    }

    /// V78: an attachment is a load event too, and it is labelled as a
    /// different KIND than a tool result so a parser can tell them apart.
    #[test]
    fn attachments_are_listed_and_kinded() {
        let out = run_on("weird.jsonl", &["--format", "json"]);
        assert!(out.out.contains("\"kind\":\"attachment\""));
        assert!(out.out.contains("\"source\":\"hook_success\""));
        // A tool result with no path reports null, never an empty string.
        assert!(out.out.contains("\"path\":null"));
    }

    /// `--format human` is the default AND spellable, like every verb.
    #[test]
    fn human_format_is_default_and_explicit() {
        let a = run_on("tool-shapes.jsonl", &[]);
        let b = run_on("tool-shapes.jsonl", &["--format", "human"]);
        assert_eq!(a.out, b.out);
    }

    #[test]
    fn a_bad_format_is_a_usage_error() {
        let out = run_on("tool-shapes.jsonl", &["--format", "yaml"]);
        assert_eq!(out.code, 2);
        assert!(out.err.contains("unknown format"));
    }

    /// A flag that needs a value and does not get one is a usage error,
    /// not a silent default.
    #[test]
    fn a_flag_missing_its_value_errors() {
        let out = run_on("tool-shapes.jsonl", &["-n"]);
        assert_eq!(out.code, 2);
        assert!(out.err.contains("needs a value"));
    }

    /// Discovery: with no session argument and no transcript for the
    /// directory, there is nothing to report -- and that is exit 0 (V5).
    #[test]
    fn no_session_and_no_transcript_is_empty() {
        let dir = std::env::temp_dir()
            .join(format!("itok-tr-none-{}", std::process::id()));
        std::fs::create_dir_all(&dir).ok();
        let out = trace(&["-C".to_owned(), dir.display().to_string()]);
        assert_eq!(out.code, 0);
        assert!(out.out.is_empty());
    }

    #[test]
    fn unknown_flags_are_usage_errors() {
        let out = run_on("tool-shapes.jsonl", &["--nope"]);
        assert_eq!(out.code, 2);
    }
}
