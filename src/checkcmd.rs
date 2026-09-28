//! The `check` command: gate registered paths against `.context-limits`
//! (V10). Committed policy -- "these files must stay under these token
//! ceilings" -- the counterpart to the inline `--budget`. The tier is
//! PINNED for a deterministic, cacheable verdict (V5): bpe when built in,
//! dummy otherwise -- both deterministic. Opt-in: only registered paths
//! are gated; everything else is unchecked (V10). Exit 1 on any breach.

use crate::args::Format;
use crate::cli::Output;
use crate::estimate;
use crate::render::Method;
use crate::units;
use std::path::{Path, PathBuf};

const LIMITS: &str = ".context-limits";
const BPE: bool = cfg!(feature = "bpe");

/// A breach: path, its token cost, and the ceiling it exceeded.
type Breach = (String, u64, u64);

pub(crate) fn check(rest: &[String]) -> Output {
    match parse(rest) {
        Ok((root, format)) => run(&root, format),
        Err(e) => Output::usage_err(format!("itok: {e}")),
    }
}

fn run(root: &Path, format: Format) -> Output {
    match read_limits(root) {
        Ok(entries) => {
            let breaches = evaluate(root, &entries);
            verdict(&tally_of(root, &entries), &breaches, format)
        }
        // A row the tool cannot read is a row the AUTHOR believes is
        // enforced, so this is a usage error, not a pass (V88/B7).
        Err(e) => Output::usage_err(format!("itok: {e}")),
    }
}

fn method() -> &'static Method {
    // cfg, not a runtime `if BPE`, so the untaken tier is not compiled --
    // otherwise it reads as a dead (uncovered) branch.
    #[cfg(feature = "bpe")]
    {
        &crate::render::O200K
    }
    #[cfg(not(feature = "bpe"))]
    {
        &crate::render::DUMMY
    }
}

/// A registered path and its ceiling.
type Entry = (String, u64);

/// The registry, or the first row that could not be read.
///
/// A MISSING file is still no enforcement (V10, opt-in); an UNREADABLE ROW
/// inside a present file is a different thing entirely.
fn read_limits(root: &Path) -> Result<Vec<Entry>, String> {
    match std::fs::read_to_string(root.join(LIMITS)) {
        Ok(t) => parse_limits(&t),
        Err(_) => Ok(Vec::new()),
    }
}

/// Parse every row, FAILING on the first unreadable one.
///
/// B7: this used to `filter_map`, so `SPEC.md 20.5k` was silently dropped
/// and `check` reported `checked:1` of 2 registered paths with exit 0. The
/// ratchet gated nothing and read as if it did -- strictly worse than
/// having no row at all (V88/V69), and V11 had already forbidden exactly
/// this for the sibling registry.
fn parse_limits(text: &str) -> Result<Vec<Entry>, String> {
    text.lines()
        .enumerate()
        .map(|(i, l)| (i.saturating_add(1), l.trim()))
        .filter(|(_, l)| !l.is_empty() && !l.starts_with('#'))
        .map(|(n, l)| entry(l).ok_or_else(|| row_err(n, l)))
        .collect()
}

/// Names the FILE, the LINE, and what was expected (V88) -- a diagnostic
/// that only says "bad row" leaves the author guessing which one.
fn row_err(line: usize, text: &str) -> String {
    format!(
        "{LIMITS}:{line}: expected `<path> <count>` \
         (count like 20000, 20k, 1M), got `{text}`"
    )
}

fn entry(line: &str) -> Option<Entry> {
    let mut p = line.split_whitespace();
    let path = p.next()?.to_owned();
    let cap = units::parse(p.next()?).ok()?;
    Some((path, cap))
}

/// What the registry actually measured: rows whose path exists (a file,
/// or a directory another tool budgets), and the rows that match NOTHING.
struct Tally {
    measured: usize,
    unmatched: Vec<String>,
}

/// #42: a row naming a missing path used to be counted "within budget",
/// so a renamed file or a typo'd row left a ceiling that could never fail
/// and a log that could not tell. It still passes (V10, opt-in), but it
/// is counted apart and named, so a gate log shows the row guards nothing.
fn tally_of(root: &Path, entries: &[Entry]) -> Tally {
    let unmatched: Vec<String> = entries
        .iter()
        .filter(|(p, _)| !root.join(p).exists())
        .map(|(p, _)| p.clone())
        .collect();
    Tally {
        measured: entries.len().saturating_sub(unmatched.len()),
        unmatched,
    }
}

/// Registered paths whose token cost exceeds their ceiling. A path absent
/// on disk counts as 0 and passes -- `Tally` is what reports it.
fn evaluate(root: &Path, entries: &[Entry]) -> Vec<Breach> {
    entries
        .iter()
        .filter_map(|(path, cap)| {
            let tokens = estimate::count(&root.join(path), BPE)?;
            (tokens > *cap).then_some((path.clone(), tokens, *cap))
        })
        .collect()
}

fn verdict(t: &Tally, breaches: &[Breach], format: Format) -> Output {
    match format {
        Format::Json => json(t, breaches),
        Format::Human => human(t, breaches),
    }
}

fn human(t: &Tally, breaches: &[Breach]) -> Output {
    let label = method().label();
    let warn = unmatched_report(&t.unmatched);
    if breaches.is_empty() {
        return Output {
            out: ok_line(t, &label),
            err: warn,
            code: 0,
        };
    }
    Output {
        out: String::new(),
        err: breach_report(&label, breaches) + &warn,
        code: 1,
    }
}

/// Unchanged when every row matched; otherwise the unmatched rows are
/// counted beside the pass rather than inside it (#42).
fn ok_line(t: &Tally, label: &str) -> String {
    let n = t.measured;
    let tail = match t.unmatched.len() {
        0 => String::new(),
        k => format!(", {k} row(s) matched nothing"),
    };
    format!("check ok: {n} path(s) within budget ({label}){tail}\n")
}

/// One stderr line per unmatched row, greppable by a gate (#42).
fn unmatched_report(unmatched: &[String]) -> String {
    unmatched
        .iter()
        .map(|p| {
            format!(
                "itok: {LIMITS}: `{p}` matches nothing on disk \
                 -- the row guards nothing\n"
            )
        })
        .collect()
}

fn breach_report(label: &str, breaches: &[Breach]) -> String {
    let mut s =
        format!("itok: {} path(s) over budget ({label}):\n", breaches.len());
    for (path, tokens, cap) in breaches {
        s.push_str(&format!("  {tokens} itok > {cap}  {path}\n"));
    }
    s
}

fn json(t: &Tally, breaches: &[Breach]) -> Output {
    let ok = breaches.is_empty();
    let out = json_body(ok, t, breaches);
    if ok {
        Output::ok(out)
    } else {
        Output {
            out,
            err: String::new(),
            code: 1,
        }
    }
}

fn json_body(ok: bool, t: &Tally, breaches: &[Breach]) -> String {
    format!(
        "{{\"ok\":{ok},\"method\":\"{}\",\"checked\":{},\"breaches\":[{}],\
         \"unmatched\":[{}]}}\n",
        method().label(),
        t.measured,
        json_items(breaches),
        json_paths(&t.unmatched),
    )
}

fn json_paths(paths: &[String]) -> String {
    paths
        .iter()
        .map(|p| format!("\"{}\"", crate::json::escape(p)))
        .collect::<Vec<_>>()
        .join(",")
}

fn json_items(breaches: &[Breach]) -> String {
    breaches
        .iter()
        .map(|(p, t, c)| {
            format!(
                "{{\"path\":\"{}\",\"tokens\":{t},\"limit\":{c}}}",
                crate::json::escape(p)
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn parse(rest: &[String]) -> Result<(PathBuf, Format), String> {
    let mut root = PathBuf::from(".");
    let mut format = Format::Human;
    let mut i = 0usize;
    while let Some(a) = rest.get(i) {
        match a.as_str() {
            "-C" => root = PathBuf::from(take(rest, &mut i)?),
            "--format" => format = fmt(&take(rest, &mut i)?)?,
            p => return Err(format!("unknown argument '{p}'")),
        }
        i = i.saturating_add(1);
    }
    Ok((root, format))
}

fn take(rest: &[String], i: &mut usize) -> Result<String, String> {
    *i = i.saturating_add(1);
    rest.get(*i)
        .cloned()
        .ok_or_else(|| "flag needs a value".to_owned())
}

fn fmt(s: &str) -> Result<Format, String> {
    match s {
        "json" => Ok(Format::Json),
        "human" => Ok(Format::Human),
        other => Err(format!("unknown format '{other}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIR: &str = env!("CARGO_MANIFEST_DIR");

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn parses_paths_and_ceilings_skipping_comments() {
        let e = parse_limits("# note\n\nSPEC.md  60k\nfoo/bar  100\n");
        assert_eq!(
            e,
            Ok(vec![
                ("SPEC.md".to_owned(), 60_000),
                ("foo/bar".to_owned(), 100)
            ])
        );
    }

    /// B7, REVERSED. This test previously asserted the bug: a malformed row
    /// was "skipped" and the registry came back empty, so `check` passed
    /// while gating nothing. It now asserts the row FAILS, and that the
    /// message names the file, the line, and the expected form (V88).
    #[test]
    fn an_unreadable_row_fails_and_says_where() {
        let e = parse_limits("# note\nSPEC.md notanumber\n").err();
        let msg = e.unwrap_or_default();
        assert!(msg.contains(".context-limits:2:"), "file and line: {msg}");
        assert!(msg.contains("expected"), "what was wanted: {msg}");
        assert!(msg.contains("notanumber"), "and what was found: {msg}");
    }

    /// B7's exact input. A fractional unit is rejected LOUDLY (T69).
    #[test]
    fn a_fractional_unit_is_rejected_not_dropped() {
        assert!(parse_limits("SPEC.md 20.5k\n").is_err());
    }

    /// A row missing its ceiling entirely also fails -- the author wrote a
    /// path expecting it to be gated.
    #[test]
    fn a_row_without_a_ceiling_fails() {
        assert!(parse_limits("SPEC.md\n").is_err());
    }

    /// V10 still holds: a MISSING registry is no enforcement, not an error.
    /// Only an unreadable row inside a present file fails.
    #[test]
    fn a_missing_registry_is_still_no_enforcement() {
        let empty = Path::new("/nonexistent-itok-root");
        assert_eq!(read_limits(empty), Ok(Vec::new()));
    }

    #[test]
    fn evaluate_flags_only_files_over_their_ceiling() {
        let root = Path::new(DIR);
        assert!(
            evaluate(root, &[("Cargo.toml".to_owned(), 9_999_999)]).is_empty()
        );
        assert_eq!(evaluate(root, &[("Cargo.toml".to_owned(), 1)]).len(), 1);
    }

    #[test]
    fn a_missing_registered_path_passes() {
        let root = Path::new(DIR);
        assert!(evaluate(root, &[("no/such/file".to_owned(), 1)]).is_empty());
    }

    /// #42: a row naming a path that does not exist guards NOTHING, and
    /// used to be counted "within budget" in the same words as a real
    /// pass. It is now named on stderr and counted apart, exit unchanged
    /// (V10: the registry stays opt-in; a directory row still counts).
    #[test]
    fn a_row_that_matches_nothing_is_counted_apart() {
        let tally = demo_tally();
        assert_eq!(tally.measured, 2, "the file and the directory");
        assert_eq!(tally.unmatched, vec!["nosuchfile.md".to_owned()]);
    }

    fn demo_tally() -> Tally {
        let rows = [
            ("Cargo.toml".to_owned(), 9_999_999),
            ("src".to_owned(), 10),
            ("nosuchfile.md".to_owned(), 5),
        ];
        tally_of(Path::new(DIR), &rows)
    }

    #[test]
    fn a_row_that_matches_nothing_is_named_in_both_formats() {
        let tally = demo_tally();
        let o = verdict(&tally, &[], Format::Human);
        assert_eq!(o.code, 0);
        assert!(o.out.contains("check ok: 2 path(s)"), "{}", o.out);
        assert!(o.out.contains("1 row(s) matched nothing"), "{}", o.out);
        assert!(o.err.contains("nosuchfile.md"), "{}", o.err);
        let j = verdict(&tally, &[], Format::Json).out;
        assert!(j.contains("\"checked\":2"), "{j}");
        assert!(j.contains("\"unmatched\":[\"nosuchfile.md\"]"), "{j}");
    }

    /// With every row matched, the summary reads exactly as it always did.
    #[test]
    fn a_fully_matched_registry_says_nothing_extra() {
        let o = verdict(&clean(3), &[], Format::Human);
        assert!(!o.out.contains("matched nothing"), "{}", o.out);
        assert!(o.err.is_empty(), "{}", o.err);
    }

    fn clean(measured: usize) -> Tally {
        Tally {
            measured,
            unmatched: Vec::new(),
        }
    }

    #[test]
    fn a_clean_verdict_exits_zero() {
        let o = verdict(&clean(3), &[], Format::Human);
        assert_eq!(o.code, 0);
        assert!(o.out.contains("within budget"));
    }

    #[test]
    fn a_breach_verdict_exits_one() {
        let b = vec![("SPEC.md".to_owned(), 99u64, 10u64)];
        let o = verdict(&clean(1), &b, Format::Human);
        assert_eq!(o.code, 1);
        assert!(o.err.contains("over budget"));
        assert!(o.err.contains("SPEC.md"));
    }

    #[test]
    fn json_reports_ok_and_breaches() {
        assert!(json(&clean(2), &[]).out.contains("\"ok\":true"));
        let b = vec![("a".to_owned(), 9u64, 1u64)];
        let o = json(&clean(1), &b);
        assert_eq!(o.code, 1);
        assert!(o.out.contains("\"ok\":false"));
    }

    #[test]
    fn check_on_itok_passes_its_own_policy() {
        if !crate::testutil::dogfood() {
            return;
        }
        // Dogfood (V14): crates/itok/.context-limits keeps itok within budget,
        // and the pass is NON-vacuous -- the registry actually gates SPEC.md,
        // not an empty registry that trivially passes (V10).
        let entries = read_limits(Path::new(DIR)).unwrap_or_default();
        assert!(
            entries.iter().any(|(p, _)| p == "SPEC.md"),
            "dogfood registry must gate SPEC.md: {entries:?}"
        );
        assert_eq!(check(&args(&["-C", DIR])).code, 0);
    }

    #[test]
    fn no_registry_is_a_pass() {
        // A dir with no .context-limits has nothing to gate (V10).
        let o = check(&args(&["-C", &format!("{DIR}/src")]));
        assert_eq!(o.code, 0);
    }

    #[test]
    fn a_bad_flag_is_a_usage_error() {
        assert_eq!(check(&args(&["--bogus"])).code, 2);
    }

    #[test]
    fn a_bad_format_is_a_usage_error() {
        assert_eq!(check(&args(&["--format", "yaml"])).code, 2);
    }

    #[test]
    fn a_flag_missing_its_value_errors() {
        assert_eq!(check(&args(&["-C"])).code, 2);
    }

    #[test]
    fn human_format_is_the_default_and_explicit() {
        assert_eq!(check(&args(&["--format", "human", "-C", DIR])).code, 0);
    }
}
