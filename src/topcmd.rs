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
        Format::Json => json(&rows, &parsed),
        Format::Human => report(&rows, &parsed, &raw.style),
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
fn report(rows: &[Row], parsed: &Session, style: &Style) -> String {
    let total: u64 = rows.iter().fold(0, |a, r| a.saturating_add(r.tokens));
    let mut out = if style.summarize {
        String::new()
    } else {
        rows.iter().map(|r| line(r, style)).collect()
    };
    out.push_str(&total_line(total, style));
    out.push_str(&ledger(parsed, style));
    out
}

/// The accounted/unaccounted split (V44). Each number names its method
/// (V3), because they do not share one: the window is EXACT, from the
/// harness's own usage record, while accounted is `bytes/4`.
///
/// The gap therefore inherits the estimate's error and keeps the tilde
/// even though one operand is exact -- subtracting an estimate from a
/// measurement yields an estimate, and rendering it as a measurement
/// would launder it.
///
/// The categories are NAMED but never apportioned: assigning bytes to
/// them would be the silent attribution V44 forbids.
fn ledger(parsed: &Session, style: &Style) -> String {
    let Some(win) = parsed.window() else {
        // No usage anywhere: report nothing rather than zero (V47).
        return String::new();
    };
    // Named arguments throughout: positional order silently transposed
    // `gap` and the load count on the first attempt, and both are
    // plausible numbers, so nothing failed -- it just printed a lie.
    format!(
        "\n{win:>9} itok  window       (actual, last turn)\n\
         {acc:>9} itok  accounted    (bytes/4, {loads} loads)\n\
         {gap:>9} itok  unaccounted  (system prompt + tool schemas + conversation)\n\
         {billed:>9} itok  billed       (actual, {turns} turns)\n",
        win = size(win, style),
        acc = tilde(parsed.accounted_tokens(), style),
        loads = parsed.events.len(),
        gap = tilde(parsed.unaccounted_tokens().unwrap_or(0), style),
        billed = size(parsed.billed_input(), style),
        turns = parsed.turns.len(),
    ) + &cache_lines(parsed, style)
}

/// How `billed` decomposes: fresh, written to cache, served from cache.
/// READ from `usage`, never inferred (V47) -- so no hit-rate percentage
/// either, which would be a judgment about whether caching is working.
/// The three numbers are arithmetic; a reader can divide.
///
/// A field absent from EVERY turn produces NO line, rather than a zero
/// that would read as "the cache was never used" (V47). Each part is
/// independent: one missing must not suppress another that is present.
fn cache_lines(parsed: &Session, style: &Style) -> String {
    [
        ("fresh", parsed.fresh_input()),
        ("cache write", parsed.cache_written()),
        ("cache read", parsed.cache_read()),
    ]
    .iter()
    .filter_map(|(label, v)| Some((label, (*v)?)))
    .map(|(label, v)| format!("{:>19} itok    {label}\n", size(v, style)))
    .collect::<String>()
        + &cold_line(parsed, style)
}

/// Turns that re-wrote the prefix instead of reading it (V95).
///
/// Reports the COST and stops. No cause: a re-auth explains the one
/// instance measured, and generalising from a sample of two would be the
/// confident guess V3 forbids. No advice either -- this is an
/// observation, and judgment belongs to `doctor` (V59). Explicitly NOT a
/// fuse input: the version of this finding that retargeted the fuse was
/// drafted, then refuted by looking at the data (V95).
fn cold_line(parsed: &Session, style: &Style) -> String {
    let Some(cold) = parsed.cold_cache() else {
        return String::new(); // cache fields absent: cannot tell (V47)
    };
    if cold.turns == 0 {
        return String::new();
    }
    format!(
        "{:>28} turn(s) re-wrote the prefix ({} written, {} read)\n",
        cold.turns,
        size(cold.written, style),
        size(cold.read, style),
    )
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
fn json(rows: &[Row], parsed: &Session) -> String {
    let mut out: String = rows.iter().map(json_object).collect();
    out.push_str(&json_summary(parsed));
    out
}

/// The split as one extra object, marked `summary` so a reader can tell
/// it from a row. Additive: existing row parsers are unaffected (V9).
/// The three counts that describe coverage rather than cost. `skipped`
/// is here because a reader must be able to see that records were
/// unreadable (V43/V44).
fn json_counts(parsed: &Session) -> String {
    format!(
        "\"loads\":{},\"turns\":{},\"skipped\":{}",
        parsed.events.len(),
        parsed.turns.len(),
        parsed.skipped,
    )
}

/// The cache decomposition, as `null` when a field was never reported.
///
/// The human view OMITS an absent line; json keeps the KEY and writes
/// `null`. Different idioms, one rule (V47): a reader must be able to
/// tell "not measured" from "measured as zero". Omitting keys
/// conditionally would make the stable contract (V9) awkward to parse,
/// while `null` is json's own word for absent.
fn json_cache(parsed: &Session) -> String {
    let field =
        |v: Option<u64>| v.map_or_else(|| "null".to_owned(), |n| n.to_string());
    format!(
        "\"fresh\":{},\"cache_write\":{},\"cache_read\":{},\
         \"cold_cache_turns\":{},\"cold_cache_written\":{}",
        field(parsed.fresh_input()),
        field(parsed.cache_written()),
        field(parsed.cache_read()),
        // null, not 0: "cannot tell" and "none found" are different
        // answers, and only one of them is a measurement (V47).
        field(parsed.cold_cache().map(|c| c.turns as u64)),
        field(parsed.cold_cache().map(|c| c.written)),
    )
}

fn json_summary(parsed: &Session) -> String {
    let Some(win) = parsed.window() else {
        return String::new();
    };
    let counts = json_counts(parsed);
    let cache = json_cache(parsed);
    format!(
        "{{\"summary\":true,\"window\":{win},\"accounted\":{acc},\
         \"unaccounted\":{gap},\"billed\":{billed},{cache},{counts},\
         \"unit\":\"input_tokens\",\"window_method\":\"actual\",\
         \"accounted_method\":\"bytes/4\"}}\n",
        acc = parsed.accounted_tokens(),
        gap = parsed.unaccounted_tokens().unwrap_or(0),
        billed = parsed.billed_input(),
    )
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

    /// The ranked rows only: everything before the total line. The
    /// ledger footer follows it and is not a row.
    fn rows_of(out: &Output) -> Vec<&str> {
        out.out
            .lines()
            .take_while(|l| !l.contains("total (bytes/4)"))
            .collect()
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
    fn summarize_drops_the_rows_but_keeps_the_ledger() {
        let out = run_on("tool-shapes.jsonl", &["-s"]);
        assert!(rows_of(&out).is_empty(), "no per-row lines under -s");
        assert!(out.out.contains("total (bytes/4)"));
        // The split is the point of the verb, so -s keeps it (V44).
        assert!(out.out.contains("unaccounted"));
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
        let human = rows_of(&run_on("tool-shapes.jsonl", &[])).len();
        let rows = json_rows(&out);
        assert_eq!(rows.len(), human, "one object per ranked row");
        for line in rows {
            assert!(line.contains("\"unit\":\"input_tokens\""));
            assert!(line.contains("\"estimated\":true"));
            assert!(line.contains("\"stale_turns\":"));
        }
        assert!(!out.out.contains('~'), "no tilde in a numeric field (V3)");
    }

    /// Row objects only -- the summary is marked and excluded.
    fn json_rows(out: &Output) -> Vec<&str> {
        out.out
            .lines()
            .filter(|l| !l.contains("\"summary\":true"))
            .collect()
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

    /// V44: the split is exact arithmetic -- accounted plus unaccounted
    /// is the window, with nothing rounded away or silently assigned.
    #[test]
    fn accounted_plus_unaccounted_is_the_window() {
        let text = std::fs::read_to_string(fixture("minimal.jsonl"))
            .unwrap_or_default();
        let s = claude_code::parse(&text);
        let win = s.window().unwrap_or(0);
        let sum = s
            .accounted_tokens()
            .saturating_add(s.unaccounted_tokens().unwrap_or(0));
        assert_eq!(sum, win);
    }

    /// The split is measured against ONE window, never the cumulative
    /// bill. This fixture bills more across its turns than any single
    /// window holds, so comparing against the cumulative would make the
    /// gap absurd -- the regression this test exists to catch (V44).
    #[test]
    fn the_split_uses_one_window_not_the_cumulative_bill() {
        let text = std::fs::read_to_string(fixture("minimal.jsonl"))
            .unwrap_or_default();
        let s = claude_code::parse(&text);
        let win = s.window().unwrap_or(0);
        assert!(
            win <= s.billed_input(),
            "a window can never exceed the cumulative bill"
        );
        assert!(
            s.unaccounted_tokens().unwrap_or(u64::MAX) <= win,
            "the gap is bounded by ONE window, not by the whole session"
        );
    }

    /// V3: the gap inherits the estimate, so it keeps the tilde even
    /// though the window it came from is exact. Laundering an estimate
    /// into a measurement is the failure this guards.
    #[test]
    fn the_gap_is_marked_estimated() {
        let out = run_on("minimal.jsonl", &["-s"]);
        let gap = out
            .out
            .lines()
            .find(|l| l.contains("unaccounted"))
            .unwrap_or_default();
        assert!(gap.contains('~'), "unaccounted must carry the tilde");
        let win = out
            .out
            .lines()
            .find(|l| l.contains("window"))
            .unwrap_or_default();
        assert!(!win.contains('~'), "the window is exact, no tilde");
    }

    /// V47: no usage anywhere means the split is ABSENT, not zero.
    #[test]
    fn no_usage_reports_no_split_rather_than_zero() {
        let out = run_on("tool-shapes.jsonl", &["-s"]);
        assert!(out.out.contains("window"), "this fixture has usage");
        let s = crate::session::Session::default();
        assert_eq!(s.window(), None);
        assert_eq!(s.unaccounted_tokens(), None);
    }

    /// `bytes/4` can over-estimate, so accounted may exceed the window.
    /// The gap clamps at zero rather than going negative, which would say
    /// something false about the data instead of about the estimator.
    ///
    /// Built in memory rather than from a fixture: no real transcript has
    /// this shape, and inventing one that did would make the fixture less
    /// representative to test an arithmetic edge.
    #[test]
    fn an_over_estimate_clamps_the_gap_at_zero() {
        let mut s = crate::session::Session::default();
        s.turns.push(crate::session::Turn {
            cache_read: Some(10),
            ..crate::session::Turn::default()
        });
        s.events.push(crate::session::LoadEvent {
            session: String::new(),
            ts: String::new(),
            source: crate::session::Source::Tool("shell".to_owned()),
            path: None,
            bytes: 4000,
            spilled: None,
        });
        assert!(s.accounted_tokens() > s.window().unwrap_or(0));
        assert_eq!(s.unaccounted_tokens(), Some(0), "clamped, never negative");
    }

    /// V9: the summary is one extra object, marked so a reader can tell
    /// it from a row, and carrying a method per figure.
    #[test]
    fn the_json_summary_is_distinguishable_and_labelled() {
        let out = run_on("minimal.jsonl", &["--format", "json"]);
        let last = out.out.lines().last().unwrap_or_default();
        assert!(last.contains("\"summary\":true"));
        assert!(last.contains("\"window_method\":\"actual\""));
        assert!(last.contains("\"accounted_method\":\"bytes/4\""));
        assert!(!out.out.contains('~'), "no tilde in json (V9)");
    }

    /// V44: the decomposition is exact -- fresh + written + read is the
    /// whole bill, with nothing rounded away.
    #[test]
    fn the_cache_parts_sum_to_billed() {
        let text = std::fs::read_to_string(fixture("minimal.jsonl"))
            .unwrap_or_default();
        let s = claude_code::parse(&text);
        let parts = s
            .fresh_input()
            .unwrap_or(0)
            .saturating_add(s.cache_written().unwrap_or(0))
            .saturating_add(s.cache_read().unwrap_or(0));
        assert_eq!(parts, s.billed_input());
    }

    /// Present fields produce lines, labelled and readable.
    #[test]
    fn cache_lines_appear_when_the_fields_are_present() {
        let out = run_on("minimal.jsonl", &["-s"]);
        for label in ["fresh", "cache write", "cache read"] {
            assert!(out.out.contains(label), "missing `{label}` line");
        }
    }

    /// V47, the one that matters: a field absent from EVERY turn produces
    /// NO line -- never a zero, which would read as "the cache was never
    /// used" when the truth is that nothing was measured.
    ///
    /// Built in memory: every fixture carries these fields, as does every
    /// real transcript characterised. This is the defensive case V47
    /// exists for, so it has to be constructed rather than found.
    #[test]
    fn absent_cache_fields_produce_no_lines_and_no_zeros() {
        let mut s = crate::session::Session::default();
        s.turns.push(crate::session::Turn {
            input: Some(100),
            ..crate::session::Turn::default()
        });
        assert_eq!(s.cache_read(), None, "never reported stays None");
        let out = ledger(&s, &Style::default());
        assert!(out.contains("fresh"), "the present field still shows");
        assert!(!out.contains("cache read"), "absent field: no line");
        assert!(!out.contains("cache write"), "absent field: no line");
    }

    /// Each part is independent: one absent field must not suppress
    /// another that IS present.
    #[test]
    fn a_missing_field_does_not_suppress_a_present_one() {
        let mut s = crate::session::Session::default();
        s.turns.push(crate::session::Turn {
            cache_read: Some(500),
            ..crate::session::Turn::default()
        });
        let out = ledger(&s, &Style::default());
        assert!(out.contains("cache read"), "present field shows");
        assert!(!out.contains("fresh"), "absent field stays absent");
    }

    /// V47/V9: json keeps the KEY and writes `null` -- a different idiom
    /// from the human view's omitted line, but the same rule. A zero here
    /// would be indistinguishable from a real measurement of zero.
    #[test]
    fn json_writes_null_for_an_absent_field_never_zero() {
        let mut s = crate::session::Session::default();
        s.turns.push(crate::session::Turn {
            input: Some(7),
            ..crate::session::Turn::default()
        });
        let out = json_cache(&s);
        assert!(out.contains("\"fresh\":7"));
        assert!(out.contains("\"cache_read\":null"), "absent -> null");
        assert!(!out.contains("\"cache_read\":0"), "never a zero");
    }

    /// The cache figures are READ from usage, so they are exact and carry
    /// no tilde -- unlike the accounted line beside them (V3/V47).
    #[test]
    fn cache_figures_are_exact_not_estimated() {
        let out = run_on("minimal.jsonl", &["-s"]);
        let line = out
            .out
            .lines()
            .find(|l| l.contains("cache read"))
            .unwrap_or_default();
        assert!(!line.contains('~'), "read from usage: no tilde");
    }

    /// The rule needs no threshold: warm turns read the whole prefix and
    /// write only the new content, so read dominates; cold, there is
    /// nothing to read. Measured, the populations sit 20x apart.
    #[test]
    fn write_over_read_classifies_a_cold_turn() {
        let mut s = crate::session::Session::default();
        s.turns.push(turn_with(1_000, 600_000)); // warm
        s.turns.push(turn_with(250_657, 20_628)); // cold
        let cold = s.cold_cache().unwrap_or_default();
        assert_eq!(cold.turns, 1, "only the write-heavy turn counts");
        assert_eq!(cold.written, 250_657);
        assert_eq!(cold.read, 20_628);
    }

    /// Equal values are NEITHER. A turn reporting zero for both is not a
    /// cold cache; it is a turn that reported nothing -- the zero-window
    /// case found in a real transcript.
    #[test]
    fn equal_values_classify_as_neither() {
        let mut s = crate::session::Session::default();
        s.turns.push(turn_with(0, 0));
        s.turns.push(turn_with(500, 500));
        assert_eq!(s.cold_cache().unwrap_or_default().turns, 0);
    }

    /// V47: absent fields mean CANNOT TELL, which is not "none found".
    /// `None` and `Some(0)` are different answers and only one of them is
    /// a measurement.
    #[test]
    fn absent_cache_fields_cannot_be_classified() {
        let mut s = crate::session::Session::default();
        s.turns.push(crate::session::Turn {
            input: Some(10),
            ..crate::session::Turn::default()
        });
        assert_eq!(s.cold_cache(), None, "cannot tell, not none-found");
        assert!(cold_line(&s, &Style::default()).is_empty());
    }

    /// No cold turns means no line -- the report stays quiet when there
    /// is nothing to report (V47).
    #[test]
    fn no_cold_turns_means_no_line() {
        let mut s = crate::session::Session::default();
        s.turns.push(turn_with(1_000, 600_000));
        assert_eq!(s.cold_cache().unwrap_or_default().turns, 0);
        assert!(cold_line(&s, &Style::default()).is_empty());
    }

    /// The figures come from `usage`, so they are exact -- no tilde. And
    /// the line states the COST only: no cause, no advice (V95/V59).
    #[test]
    fn the_cold_line_reports_cost_without_cause_or_advice() {
        let mut s = crate::session::Session::default();
        s.turns.push(turn_with(250_657, 20_628));
        let out = cold_line(&s, &Style::default());
        assert!(out.contains("re-wrote the prefix"));
        assert!(!out.contains('~'), "read from usage: exact");
        for word in ["login", "auth", "avoid", "should", "consider"] {
            assert!(
                !out.to_lowercase().contains(word),
                "no cause/advice: {word}"
            );
        }
    }

    /// V47/V9: json says `null` when the fields were never reported.
    #[test]
    fn json_cold_fields_are_null_when_unclassifiable() {
        let mut s = crate::session::Session::default();
        s.turns.push(crate::session::Turn {
            input: Some(1),
            ..crate::session::Turn::default()
        });
        let out = json_cache(&s);
        assert!(out.contains("\"cold_cache_turns\":null"));
        assert!(!out.contains("\"cold_cache_turns\":0"));
    }

    fn turn_with(write: u64, read: u64) -> crate::session::Turn {
        crate::session::Turn {
            cache_creation: Some(write),
            cache_read: Some(read),
            ..crate::session::Turn::default()
        }
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
