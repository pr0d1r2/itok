//! `doctor --ollama` (V22): live model/window discovery. With `--model`,
//! the fit-line uses the model's LIVE context window from the endpoint
//! (the live endpoint beats the static table, V22); without a model, it
//! enumerates every model the host serves and reports each one's fit-% for
//! this fileset -- the answer no static table gives. Report-only; a hard
//! network failure is exit 7 (V22/V23). The token count is the offline
//! tier (dummy/`--bpe`, cheap): the discovery is the live part, not a
//! per-file generate against every model (V36 -- a margin'd estimate vs a
//! live window is what a fit check needs).
#![cfg(feature = "ollama")]

use crate::args::Opts;
use crate::cli::Output;
use crate::estimate::{count, select};
use crate::ollama;
use std::collections::BTreeMap;
use std::path::Path;

pub(crate) fn run(opts: &Opts, root: &Path) -> Output {
    let fleet = match ollama::hosts::bases(opts.ollama_hosts.as_deref(), root) {
        Ok(f) => f,
        Err(e) => return net_err(&e),
    };
    let tokens = fileset_tokens(opts, root);
    match &opts.model {
        Some(m) => fleet_model(&fleet, m, tokens),
        None => fleet_all(&fleet, tokens),
    }
}

/// How a requested `--model` resolved against what the fleet serves.
enum Resolved<'a> {
    One(&'a str),
    Ambiguous(Vec<&'a str>),
    Missing,
}

/// Resolve a requested name against the served set (V6's rule, on a
/// DISCOVERED set instead of the verb table).
///
/// Exact first, then ollama's OWN `name` = `name:latest`, then a unique
/// prefix. Step two matters: `ollama run codestral` means
/// `codestral:latest`, so meaning anything else by the same token would
/// invoke that prior and then violate it -- V2's expensive failure. The
/// prefix rung extends past ollama's grammar rather than bending it.
///
/// Never silent-picks: several hits is Ambiguous, and the caller names
/// the candidates.
fn resolve<'a>(names: &[&'a str], want: &str) -> Resolved<'a> {
    if let Some(n) = named(names, want) {
        return Resolved::One(n);
    }
    let hits: Vec<&'a str> = names
        .iter()
        .copied()
        .filter(|n| n.starts_with(want))
        .collect();
    match hits.as_slice() {
        [one] => Resolved::One(one),
        [] => Resolved::Missing,
        _ => Resolved::Ambiguous(hits),
    }
}

/// The two rungs that are NOT inference: a fully spelled name, then
/// ollama's own `name` = `name:latest`. Checked in that order so a fleet
/// carrying both `foo` and `foo:latest` resolves `foo` to itself rather
/// than to whichever the list happened to hold first.
fn named<'a>(names: &[&'a str], want: &str) -> Option<&'a str> {
    let latest = format!("{want}:latest");
    names
        .iter()
        .copied()
        .find(|n| *n == want)
        .or_else(|| names.iter().copied().find(|n| *n == latest))
}

/// One named model across the fleet: "does this fit, anywhere?" (V22/V24).
///
/// Enumerates ONCE and matches against what is actually served, rather
/// than asking each host for a literal name. That is what lets a prefix
/// resolve at all, and what lets both error paths NAME the alternatives
/// -- the old message reported `no fleet host serves 'gpt-oss'` while
/// saying nothing about the models sitting right there (V71).
fn fleet_model(fleet: &[String], want: &str, tokens: u64) -> Output {
    let windows = fleet_windows(fleet);
    if windows.is_empty() {
        return net_err("no models on the ollama fleet");
    }
    let names: Vec<&str> = windows.keys().map(String::as_str).collect();
    match resolve(&names, want) {
        Resolved::One(m) => {
            Output::ok(row(m, tokens, windows.get(m).copied().flatten()) + "\n")
        }
        Resolved::Ambiguous(c) => arg_err(&ambiguous_msg(want, &c)),
        Resolved::Missing => arg_err(&missing_msg(want, &names)),
    }
}

fn ambiguous_msg(want: &str, candidates: &[&str]) -> String {
    format!("'{want}' is ambiguous: {}", candidates.join(", "))
}

fn missing_msg(want: &str, served: &[&str]) -> String {
    format!(
        "no fleet host serves '{want}' -- served: {}",
        served.join(", ")
    )
}

/// A bad `--model` is a USAGE error, not a network one. The fleet
/// answered; the argument is what is wrong. Exit 7 means the network
/// failed, and returning it here would make a CI retry loop spin on a
/// typo -- the exit table already says 2 for usage.
fn arg_err(msg: &str) -> Output {
    Output::usage(format!("itok: {msg}\n"))
}

/// The UNION of models across the fleet (V24), each with its fit-%: the
/// answer no static table gives. A model's window comes from the first
/// host that answers; a host that is down is skipped, not fatal.
fn fleet_all(fleet: &[String], tokens: u64) -> Output {
    let windows = fleet_windows(fleet);
    if windows.is_empty() {
        return net_err("no models on the ollama fleet");
    }
    let mut s = format!("context: {tokens} itok\n");
    for (model, window) in &windows {
        s.push_str(&row(model, tokens, *window));
        s.push('\n');
    }
    Output::ok(s)
}

/// model -> window across the fleet, first host that answers winning; a
/// down host is skipped (V24 union, resilient).
fn fleet_windows(fleet: &[String]) -> BTreeMap<String, Option<u64>> {
    let mut windows = BTreeMap::new();
    for base in fleet {
        let Ok(models) = ollama::models(base) else {
            continue;
        };
        for m in models {
            windows
                .entry(m.clone())
                .or_insert_with(|| ollama::window(base, &m).ok());
        }
    }
    windows
}

/// One fit row, or a note when the model's window did not resolve.
fn row(model: &str, tokens: u64, window: Option<u64>) -> String {
    match window {
        Some(w) => fit_line(model, tokens, w),
        None => format!("  {model}  window unavailable"),
    }
}

/// The fileset's token cost for the fit check. Counts on the bpe proxy
/// when built (a fit gauge wants the honest estimate, V36), independent of
/// the `--ollama` tier -- `doctor --ollama` needs no separate `--bpe`.
fn fileset_tokens(opts: &Opts, root: &Path) -> u64 {
    select(opts, root)
        .iter()
        .filter_map(|f| count(&root.join(f), cfg!(feature = "bpe")))
        .fold(0u64, u64::saturating_add)
}

fn fit_line(model: &str, tokens: u64, window: u64) -> String {
    let p = pct(tokens, window);
    format!("  {model}  {tokens} / {window}  {p}%  {}", verdict(p))
}

fn pct(part: u64, whole: u64) -> u64 {
    part.saturating_mul(100).checked_div(whole).unwrap_or(0)
}

fn verdict(pct: u64) -> &'static str {
    if pct > 100 {
        "OVER"
    } else if pct >= 80 {
        "warn"
    } else {
        "ok"
    }
}

fn net_err(msg: &str) -> Output {
    Output {
        out: String::new(),
        err: format!("itok: {msg}\n"),
        code: 7,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pct_is_division_safe() {
        assert_eq!(pct(50, 200), 25);
        assert_eq!(pct(5, 0), 0);
    }

    #[test]
    fn verdict_bands() {
        assert_eq!(verdict(10), "ok");
        assert_eq!(verdict(90), "warn");
        assert_eq!(verdict(120), "OVER");
    }

    #[test]
    fn fit_line_names_model_tokens_and_window() {
        let l = fit_line("qwen3-coder:30b", 1000, 262_144);
        assert!(l.contains("qwen3-coder:30b"));
        assert!(l.contains("1000 / 262144"));
        assert!(l.contains("ok"));
    }

    #[test]
    fn a_network_failure_is_exit_seven() {
        assert_eq!(net_err("boom").code, 7);
    }

    const FLEET: [&str; 4] = [
        "codestral:latest",
        "gpt-oss:20b",
        "qwen3-coder:30b",
        "qwen3-coder:7b",
    ];

    fn one(want: &str) -> Option<&'static str> {
        match resolve(&FLEET, want) {
            Resolved::One(m) => Some(m),
            _ => None,
        }
    }

    /// The ask: a family name narrows to the one model serving it.
    #[test]
    fn a_unique_prefix_resolves() {
        assert_eq!(one("gpt-oss"), Some("gpt-oss:20b"));
        assert_eq!(one("gpt"), Some("gpt-oss:20b"));
    }

    /// An exact name still wins outright -- the prefix rung never gets to
    /// second-guess a fully spelled model.
    #[test]
    fn an_exact_name_wins() {
        assert_eq!(one("gpt-oss:20b"), Some("gpt-oss:20b"));
        assert_eq!(one("qwen3-coder:7b"), Some("qwen3-coder:7b"));
    }

    /// V2: `ollama run codestral` means `codestral:latest`, so the same
    /// token must mean that here. Bare `codestral` is ALSO a prefix of
    /// `codestral:latest`, so this passes either way -- the case that
    /// proves the rule is the one below, where the two rungs disagree.
    #[test]
    fn a_bare_name_means_latest_as_ollama_means_it() {
        assert_eq!(one("codestral"), Some("codestral:latest"));
    }

    /// Where `:latest` and the prefix rung DISAGREE, ollama's grammar
    /// wins: `qwen3-coder` prefixes two models, but a `qwen3-coder:latest`
    /// on the fleet is what ollama itself would run.
    #[test]
    fn latest_beats_an_otherwise_ambiguous_prefix() {
        let with_latest =
            ["qwen3-coder:latest", "qwen3-coder:30b", "qwen3-coder:7b"];
        let got = match resolve(&with_latest, "qwen3-coder") {
            Resolved::One(m) => Some(m),
            _ => None,
        };
        assert_eq!(got, Some("qwen3-coder:latest"));
    }

    /// V6: several hits ERROR with the candidates, never a silent pick.
    #[test]
    fn an_ambiguous_prefix_names_its_candidates() {
        let got = match resolve(&FLEET, "qwen") {
            Resolved::Ambiguous(c) => c,
            _ => Vec::new(),
        };
        assert_eq!(got, vec!["qwen3-coder:30b", "qwen3-coder:7b"]);
    }

    #[test]
    fn an_unserved_name_is_missing() {
        assert!(matches!(resolve(&FLEET, "llama"), Resolved::Missing));
        assert!(matches!(resolve(&[], "gpt-oss"), Resolved::Missing));
    }

    /// V71: both failure paths NAME the alternatives. The old message
    /// said only that the model was absent, while the fleet's models sat
    /// right there unmentioned.
    #[test]
    fn both_failures_name_what_is_served_and_are_usage_errors() {
        let missing = arg_err("no fleet host serves 'x' -- served: a, b");
        assert_eq!(missing.code, 2, "the fleet answered: not a network error");
        assert!(missing.err.contains("served: a, b"));
        let ambiguous = arg_err("'q' is ambiguous: q:1, q:2");
        assert_eq!(ambiguous.code, 2);
        assert!(ambiguous.err.contains("q:1, q:2"));
    }
}
