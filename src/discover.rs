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

/// One named model across the fleet: its live window from the first host
/// that serves it -- "does this fit, anywhere?" (V22/V24).
fn fleet_model(fleet: &[String], model: &str, tokens: u64) -> Output {
    for base in fleet {
        if let Ok(w) = ollama::window(base, model) {
            return Output::ok(format!("{}\n", fit_line(model, tokens, w)));
        }
    }
    net_err(&format!("no fleet host serves '{model}'"))
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
}
