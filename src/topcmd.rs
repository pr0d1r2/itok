//! The `top` command: ranked context occupancy (V46). `du`/`top`-shaped
//! -- `-h`, `-s`, `--top N` mean what they mean there (V1). Per-path
//! attribution is `top -- <path>`, which is why there is no `blame` verb:
//! `git blame` is per-LINE authorship, and the almost-match would cost
//! more than the flag (V2).
//!
//! Report-only, exit 0 (V5). Every column is ARITHMETIC -- how much, how
//! often, how long ago. Whether that is healthy is judgment, and judgment
//! belongs to `doctor` (V59).
//!
//! `stale` is turns-since-last-load, deliberately NOT `bytes x turns`.
//! The multiplication is the compounding-cost story, but it assumes every
//! one of those turns re-sent the item, which the transcript does not
//! record per item. The count is observed; the product would be a model
//! (V3). The reader can multiply.
//!
//! Sizes are `bytes/4` estimates for the same reason as `trace`: no
//! content is retained (V45), so there is nothing for a real tokenizer to
//! count.

use crate::args::Format;
use crate::cli::Output;
use crate::json::escape;
use crate::render::{human, Style};
use crate::session::{claude_code, LoadEvent, Session};
use crate::tracecmd::value;
use std::collections::BTreeMap;

#[derive(Default)]
struct Raw {
    session: Option<String>,
    path: Option<String>,
    top: Option<String>,
    style: Style,
    format: Format,
    chdir: Option<String>,
}

/// One ranked line: what was loaded, how much, how often, how stale.
struct Row {
    what: String,
    tokens: u64,
    loads: usize,
    stale: usize,
}

pub(crate) fn top(rest: &[String]) -> Output {
    match parse(rest) {
        Ok(raw) => run(&raw),
        Err(e) => Output::usage_err(format!("itok: {e}")),
    }
}

fn run(raw: &Raw) -> Output {
    let Some(path) = crate::tracecmd::source_path(
        raw.session.as_deref(),
        raw.chdir.as_deref(),
    ) else {
        return Output::ok(String::new());
    };
    // Unreadable is NOT the same as empty: printing a zero total would
    // read as a measurement of nothing, when in fact nothing was read
    // (V47's rule -- absent must stay distinguishable from zero).
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Output::ok(String::new());
    };
    let parsed = claude_code::parse(crate::session::complete_prefix(&text));
    let rows = rank(&parsed, raw);
    Output::ok(match raw.format {
        Format::Json => json(&rows),
        Format::Human => report(&rows, &raw.style),
    })
}

/// Group, then order by occupancy. `--top N` truncates AFTER ranking, so
/// it keeps the biggest rather than the first (V46, `du --top`'s intent).
fn rank(parsed: &Session, raw: &Raw) -> Vec<Row> {
    let events = filtered(&parsed.events, raw.path.as_deref());
    let mut rows = group(&events, &turn_times(parsed));
    rows.sort_by(|a, b| b.tokens.cmp(&a.tokens).then(a.what.cmp(&b.what)));
    if let Some(n) = raw.top.as_ref().and_then(|n| n.parse::<usize>().ok()) {
        rows.truncate(n);
    }
    rows
}

/// `-- <path>` restricts to one path's loads: per-path attribution
/// without a second verb (V46).
fn filtered<'a>(
    events: &'a [LoadEvent],
    want: Option<&str>,
) -> Vec<&'a LoadEvent> {
    events
        .iter()
        .filter(|e| want.is_none_or(|p| e.path.as_deref() == Some(p)))
        .collect()
}

/// Turn timestamps, ascending -- the ruler `stale` is measured against.
fn turn_times(parsed: &Session) -> Vec<String> {
    let mut ts: Vec<String> =
        parsed.turns.iter().map(|t| t.ts.clone()).collect();
    ts.sort();
    ts
}

/// Sum by identity: the path when a load names one, else the source in
/// parens. One column, one meaning -- a shell command genuinely has no
/// path, and inventing one would be a confident lie (V3).
fn group(events: &[&LoadEvent], turns: &[String]) -> Vec<Row> {
    let mut acc: BTreeMap<String, Sum> = BTreeMap::new();
    for e in events {
        acc.entry(identity(e)).or_default().absorb(e);
    }
    acc.into_iter()
        .map(|(what, s)| Row {
            what,
            tokens: s.tokens,
            loads: s.loads,
            stale: turns_after(turns, &s.last),
        })
        .collect()
}

/// A load's identity: its path, or its source in parens when it has none.
fn identity(e: &LoadEvent) -> String {
    e.path
        .clone()
        .unwrap_or_else(|| format!("({})", e.source.label()))
}

/// Running totals for one identity.
#[derive(Default)]
struct Sum {
    tokens: u64,
    loads: usize,
    last: String,
}

impl Sum {
    fn absorb(&mut self, e: &LoadEvent) {
        self.tokens = self.tokens.saturating_add(tokens_of(e.bytes));
        self.loads = self.loads.saturating_add(1);
        if e.ts > self.last {
            self.last.clone_from(&e.ts);
        }
    }
}

/// `bytes/4` -- the only tier available without content (V45).
fn tokens_of(bytes: usize) -> u64 {
    u64::try_from(bytes / 4).unwrap_or(u64::MAX)
}

/// Turns that happened after this thing was last loaded. ISO-8601 sorts
/// lexicographically, so a string comparison is the right one.
fn turns_after(turns: &[String], last: &str) -> usize {
    turns.iter().filter(|t| t.as_str() > last).count()
}

/// The total covers the SHOWN rows, so `--top N` narrows it -- matching
/// `estimate --top N`, which does the same. Consistency beats a
/// standalone improvement here: two verbs whose `--top` meant different
/// things would be the near-collision V2 warns about.
fn report(rows: &[Row], style: &Style) -> String {
    let total: u64 = rows.iter().fold(0, |a, r| a.saturating_add(r.tokens));
    if style.summarize {
        return total_line(total, style);
    }
    let mut out: String = rows.iter().map(|r| line(r, style)).collect();
    out.push_str(&total_line(total, style));
    out
}

fn total_line(total: u64, style: &Style) -> String {
    format!("{:>8} itok  total (bytes/4)\n", tilde(total, style))
}

fn line(r: &Row, style: &Style) -> String {
    format!(
        "{:>6}  {:>8} itok  {:>6}  {}\n",
        r.loads,
        tilde(r.tokens, style),
        r.stale,
        r.what
    )
}

/// The crude-tier marker sits AGAINST its number (`~18314`), the way
/// every other verb renders it -- a gap would be a near-collision with
/// the established look (V2/V3).
fn tilde(n: u64, style: &Style) -> String {
    format!("~{}", size(n, style))
}

fn size(n: u64, style: &Style) -> String {
    if style.human {
        human(n)
    } else {
        n.to_string()
    }
}

/// One JSON object per row (V9), same field vocabulary as every other
/// verb so a parser learns it once.
fn json(rows: &[Row]) -> String {
    rows.iter().map(json_object).collect()
}

fn json_object(r: &Row) -> String {
    format!(
        "{{\"what\":\"{}\",\"tokens\":{},\"loads\":{},\"stale_turns\":{},\
         \"unit\":\"input_tokens\",\"estimated\":true,\"method\":\"bytes/4\"}}\n",
        escape(&r.what),
        r.tokens,
        r.loads,
        r.stale,
    )
}

fn parse(rest: &[String]) -> Result<Raw, String> {
    let mut raw = Raw::default();
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        apply(a, &mut it, &mut raw)?;
    }
    Ok(raw)
}

/// One argument. `-h`/`-s`/`--top` are `du`'s, `--` is git's path
/// separator, and a bare word is the session -- nothing new to learn (V1).
fn apply<'a>(
    a: &str,
    it: &mut impl Iterator<Item = &'a String>,
    raw: &mut Raw,
) -> Result<(), String> {
    match a {
        "-h" => raw.style.human = true,
        "-s" => raw.style.summarize = true,
        "--bpe" | "--ollama" => {
            return Err(crate::tracecmd::no_real_tier(a, "top"))
        }
        _ => return with_value(a, it, raw),
    }
    Ok(())
}

/// Flags that consume the next argument, plus the fallthrough cases.
fn with_value<'a>(
    a: &str,
    it: &mut impl Iterator<Item = &'a String>,
    raw: &mut Raw,
) -> Result<(), String> {
    match a {
        "--top" => raw.top = Some(value(it, a)?),
        "-C" => raw.chdir = Some(value(it, a)?),
        "--" => raw.path = Some(value(it, a)?),
        "--format" => raw.format = crate::tracecmd::format_of(&value(it, a)?)?,
        other if other.starts_with('-') => {
            return Err(format!("unknown flag {other}"))
        }
        other => raw.session = Some(other.to_owned()),
    }
    Ok(())
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
        top(&args)
    }

    fn rows_of(out: &Output) -> Vec<&str> {
        out.out.lines().filter(|l| !l.contains("total")).collect()
    }

    /// V46: ranked by occupancy, biggest first.
    #[test]
    fn rows_are_ranked_by_occupancy() {
        let out = run_on("tool-shapes.jsonl", &[]);
        let nums: Vec<u64> = rows_of(&out)
            .iter()
            .filter_map(|l| {
                l.split('~').nth(1)?.split_whitespace().next()?.parse().ok()
            })
            .collect();
        let mut sorted = nums.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(nums, sorted, "rows must descend by tokens");
    }

    /// V46: `--top N` keeps the BIGGEST N, not the first N.
    #[test]
    fn top_n_truncates_the_ranking() {
        let all = run_on("tool-shapes.jsonl", &[]);
        let two = run_on("tool-shapes.jsonl", &["--top", "2"]);
        assert!(rows_of(&two).len() < rows_of(&all).len());
        assert_eq!(rows_of(&two).len(), 2);
    }

    /// `-s` collapses to the total line, like `du -s`.
    #[test]
    fn summarize_is_the_total_only() {
        let out = run_on("tool-shapes.jsonl", &["-s"]);
        assert_eq!(out.out.lines().count(), 1);
        assert!(out.out.contains("total (bytes/4)"));
    }

    /// `-h` abbreviates, like `du -h`. Tested on the formatter directly:
    /// no fixture is large enough to abbreviate, and a test that passes
    /// only because both sides are small asserts nothing.
    #[test]
    fn human_sizes_abbreviate() {
        let plain = Style::default();
        let big = Style {
            human: true,
            summarize: false,
        };
        assert_eq!(tilde(12_345, &plain), "~12345");
        assert_eq!(tilde(12_345, &big), "~12k");
    }

    /// V46: per-path attribution is `top -- <path>`, which is why there is
    /// no `blame` verb.
    #[test]
    fn a_path_filter_narrows_to_one_paths_loads() {
        let out = run_on("tool-shapes.jsonl", &["--", "/w/one.txt"]);
        let rows = rows_of(&out);
        assert_eq!(rows.len(), 1, "one row for one path");
        assert!(rows.first().is_some_and(|r| r.contains("/w/one.txt")));
    }

    /// V59: the dup count is arithmetic -- a path loaded twice says 2.
    /// `/w/one.txt` is read and then edited in the fixture.
    #[test]
    fn repeated_loads_are_counted() {
        let out = run_on("tool-shapes.jsonl", &["--", "/w/one.txt"]);
        let row = rows_of(&out).first().copied().unwrap_or_default();
        let loads: usize = row
            .split_whitespace()
            .next()
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        assert_eq!(loads, 2, "read + edit = two loads of the same path");
    }

    /// A load with no path is grouped under its source, in parens -- never
    /// under an invented path (V3).
    #[test]
    fn pathless_loads_group_under_their_source() {
        let out = run_on("tool-shapes.jsonl", &[]);
        assert!(out.out.contains("(shell)"));
    }

    /// V9: one JSON object per row, same field vocabulary as every verb.
    #[test]
    fn json_is_one_object_per_row() {
        let out = run_on("tool-shapes.jsonl", &["--format", "json"]);
        assert_eq!(
            out.out.lines().count(),
            rows_of(&run_on("tool-shapes.jsonl", &[])).len()
        );
        for line in out.out.lines() {
            assert!(line.contains("\"unit\":\"input_tokens\""));
            assert!(line.contains("\"estimated\":true"));
            assert!(line.contains("\"stale_turns\":"));
        }
        assert!(!out.out.contains('~'), "no tilde in a numeric field (V3)");
    }

    /// V3/V45: a real tier is refused with its reason, never ignored.
    #[test]
    fn a_real_tier_is_refused() {
        let out = run_on("tool-shapes.jsonl", &["--bpe"]);
        assert_eq!(out.code, 2);
        assert!(out.err.contains("no content is stored"));
        assert!(out.err.contains("top"), "the message names the verb");
    }

    /// V5: report-only. Nothing to read is not an error.
    #[test]
    fn a_missing_transcript_is_empty_and_exit_zero() {
        let out = top(&["/nonexistent/s.jsonl".to_owned()]);
        assert_eq!(out.code, 0);
        assert!(out.out.is_empty());
    }

    #[test]
    fn unknown_flags_and_bad_formats_are_usage_errors() {
        assert_eq!(run_on("tool-shapes.jsonl", &["--nope"]).code, 2);
        assert_eq!(run_on("tool-shapes.jsonl", &["--format", "yaml"]).code, 2);
        assert_eq!(run_on("tool-shapes.jsonl", &["--top"]).code, 2);
    }

    /// `stale` counts turns AFTER the last load. The fixture's turns all
    /// precede or interleave its events, so the newest load has none after
    /// it.
    #[test]
    fn stale_counts_turns_after_the_last_load() {
        let out = run_on("tool-shapes.jsonl", &["--", "/w/two.txt"]);
        let row = rows_of(&out).first().copied().unwrap_or_default();
        let stale: usize = row
            .split_whitespace()
            .nth(3)
            .and_then(|n| n.parse().ok())
            .unwrap_or(999);
        assert_eq!(stale, 0, "the last-loaded path has no turns after it");
    }
}
