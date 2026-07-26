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
use crate::ollama::pick::{self, Resolved};
use std::collections::BTreeMap;
use std::path::Path;

pub(crate) fn run(opts: &Opts, root: &Path) -> Output {
    let fleet = match ollama::hosts::bases(opts.ollama_hosts.as_deref(), root) {
        Ok(f) => f,
        Err(e) => return net_err(&e),
    };
    // A directory / unreadable path is the user's error, not the
    // network's, so it keeps exit 2 rather than becoming a net_err (B11d).
    let tokens = match fileset_tokens(opts, root) {
        Ok(t) => t,
        Err(e) => return arg_err(&e),
    };
    match &opts.model {
        Some(m) => fleet_model(&fleet, m, tokens),
        None => fleet_all(&fleet, tokens),
    }
}

/// One named model across the fleet: "does this fit, anywhere?" (V22/V24).
///
/// Enumerates ONCE and matches against what is actually served, rather
/// than asking each host for a literal name. That is what lets a prefix
/// resolve at all, and what lets both error paths NAME the alternatives
/// -- the old message reported `no fleet host serves 'gpt-oss'` while
/// saying nothing about the models sitting right there (V71).
fn fleet_model(fleet: &[String], want: &str, tokens: u64) -> Output {
    let seen = fleet_windows(fleet);
    if seen.windows.is_empty() {
        return net_err("no models on the ollama fleet");
    }
    let names: Vec<&str> = seen.windows.keys().map(String::as_str).collect();
    let hosts = (seen.answered, fleet.len());
    match pick::resolve(&names, want) {
        Resolved::One(m) => Output::ok(
            row(m, tokens, seen.windows.get(m).copied().flatten()) + "\n",
        ),
        Resolved::Ambiguous(c) => arg_err(&pick::ambiguous_msg(want, &c)),
        Resolved::Missing => {
            arg_err(&pick::missing_msg(want, &names, hosts.0, hosts.1))
        }
    }
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
    let seen = fleet_windows(fleet);
    if seen.windows.is_empty() {
        return net_err("no models on the ollama fleet");
    }
    let mut s = format!("context: {tokens} itok\n");
    for (model, window) in &seen.windows {
        s.push_str(&row(model, tokens, *window));
        s.push('\n');
    }
    s.push_str(&coverage(seen.answered, fleet.len()));
    Output::ok(s)
}

/// Says so when a named host did NOT answer.
///
/// B11b: this union was byte-identical whether every host replied or only
/// one did, so a partial answer read as a whole one. Silence is only safe
/// when nothing was missed -- the line appears when it was (V44). Still
/// not fatal: V24's resilience is right, its silence was not.
fn coverage(answered: usize, named: usize) -> String {
    if answered >= named {
        return String::new();
    }
    format!(
        "  note: {answered}/{named} host(s) answered -- this union is \
         PARTIAL; a model may live on a host that did not reply\n"
    )
}

/// model -> window across the fleet, first host that answers winning; a
/// down host is skipped (V24 union, resilient).
fn fleet_windows(fleet: &[String]) -> Seen {
    let mut seen = Seen::default();
    for base in fleet {
        let Ok(models) = ollama::models(base) else {
            continue; // down host: skipped, and COUNTED (V44)
        };
        seen.answered = seen.answered.saturating_add(1);
        for m in models {
            seen.windows
                .entry(m.clone())
                .or_insert_with(|| ollama::window(base, &m).ok());
        }
    }
    seen
}

/// What the fleet actually told us, and how much of the fleet told us.
/// The count travels WITH the data so a caller cannot report the union
/// without being able to say how complete it is (B11b).
#[derive(Default)]
struct Seen {
    windows: BTreeMap<String, Option<u64>>,
    answered: usize,
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
fn fileset_tokens(opts: &Opts, root: &Path) -> Result<u64, String> {
    Ok(select(opts, root)?
        .iter()
        .filter_map(|f| count(&root.join(f), cfg!(feature = "bpe")))
        .fold(0u64, u64::saturating_add))
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
