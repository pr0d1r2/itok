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
    let entries = read_limits(root);
    let breaches = evaluate(root, &entries);
    verdict(entries.len(), &breaches, format)
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

fn read_limits(root: &Path) -> Vec<(String, u64)> {
    std::fs::read_to_string(root.join(LIMITS))
        .map(|t| parse_limits(&t))
        .unwrap_or_default()
}

fn parse_limits(text: &str) -> Vec<(String, u64)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(entry)
        .collect()
}

fn entry(line: &str) -> Option<(String, u64)> {
    let mut p = line.split_whitespace();
    let path = p.next()?.to_owned();
    let cap = units::parse(p.next()?).ok()?;
    Some((path, cap))
}

/// Registered paths whose token cost exceeds their ceiling. A path absent
/// on disk counts as 0 and passes.
fn evaluate(root: &Path, entries: &[(String, u64)]) -> Vec<Breach> {
    entries
        .iter()
        .filter_map(|(path, cap)| {
            let tokens = estimate::count(&root.join(path), BPE)?;
            (tokens > *cap).then_some((path.clone(), tokens, *cap))
        })
        .collect()
}

fn verdict(n: usize, breaches: &[Breach], format: Format) -> Output {
    match format {
        Format::Json => json(n, breaches),
        Format::Human => human(n, breaches),
    }
}

fn human(n: usize, breaches: &[Breach]) -> Output {
    let label = method().label;
    if breaches.is_empty() {
        let out = format!("check ok: {n} path(s) within budget ({label})\n");
        return Output::ok(out);
    }
    Output {
        out: String::new(),
        err: breach_report(label, breaches),
        code: 1,
    }
}

fn breach_report(label: &str, breaches: &[Breach]) -> String {
    let mut s =
        format!("itok: {} path(s) over budget ({label}):\n", breaches.len());
    for (path, tokens, cap) in breaches {
        s.push_str(&format!("  {tokens} itok > {cap}  {path}\n"));
    }
    s
}

fn json(n: usize, breaches: &[Breach]) -> Output {
    let ok = breaches.is_empty();
    let out = format!(
        "{{\"ok\":{ok},\"method\":\"{}\",\"checked\":{n},\"breaches\":[{}]}}\n",
        method().label,
        json_items(breaches),
    );
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
            vec![("SPEC.md".to_owned(), 60_000), ("foo/bar".to_owned(), 100)]
        );
    }

    #[test]
    fn a_malformed_line_is_skipped() {
        assert!(parse_limits("SPEC.md notanumber\n").is_empty());
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

    #[test]
    fn a_clean_verdict_exits_zero() {
        let o = verdict(3, &[], Format::Human);
        assert_eq!(o.code, 0);
        assert!(o.out.contains("within budget"));
    }

    #[test]
    fn a_breach_verdict_exits_one() {
        let b = vec![("SPEC.md".to_owned(), 99u64, 10u64)];
        let o = verdict(1, &b, Format::Human);
        assert_eq!(o.code, 1);
        assert!(o.err.contains("over budget"));
        assert!(o.err.contains("SPEC.md"));
    }

    #[test]
    fn json_reports_ok_and_breaches() {
        assert!(json(2, &[]).out.contains("\"ok\":true"));
        let b = vec![("a".to_owned(), 9u64, 1u64)];
        let o = json(1, &b);
        assert_eq!(o.code, 1);
        assert!(o.out.contains("\"ok\":false"));
    }

    #[test]
    fn check_on_itok_passes_its_own_policy() {
        // Dogfood (V14): crates/itok/.context-limits keeps itok within budget,
        // and the pass is NON-vacuous -- the registry actually gates SPEC.md,
        // not an empty registry that trivially passes (V10).
        let entries = read_limits(Path::new(DIR));
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
