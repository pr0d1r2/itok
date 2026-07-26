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
        Some(m) => fleet_models(&fleet, m, tokens),
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
/// `--model a,b` narrows to those models (T85).
///
/// ALL-OR-NOTHING: one unresolvable element fails the whole call, naming
/// it. Reporting the elements that did resolve while staying silent about
/// one that did not is the partial-answer-reading-as-whole failure this
/// verb was just fixed for (B11b/V44) -- so the rule is applied before
/// shipping it, not after.
///
/// Comma is already the fleet grammar for HOSTS (V24), so models reuse it:
/// one list grammar to learn, not two (V1/V18's reasoning).
fn fleet_models(fleet: &[String], want: &str, tokens: u64) -> Output {
    let names: Vec<&str> = want.split(',').map(str::trim).collect();
    match names.as_slice() {
        [one] => fleet_model(fleet, one, tokens),
        _ => fleet_list(fleet, &names, tokens),
    }
}

/// Several named models, rendered like the fleet view: a shared `context:`
/// header over one fit row each, because the header carries the denominator
/// they are all compared against.
fn fleet_list(fleet: &[String], names: &[&str], tokens: u64) -> Output {
    let seen = fleet_windows(fleet);
    if seen.windows.is_empty() {
        return net_err("no models on the ollama fleet");
    }
    let fl = Fleet::of(&seen, fleet.len());
    match fl.rows_for(names, tokens) {
        Ok(rows) => Output::ok(listing(tokens, &rows, &seen, fleet.len())),
        Err(e) => arg_err(&e),
    }
}

/// The fleet view's body: a shared `context:` header over one row each,
/// because the header carries the denominator they are all compared against.
fn listing(tokens: u64, rows: &str, seen: &Seen, named: usize) -> String {
    format!("context: {tokens} itok\n{rows}") + &coverage(seen.answered, named)
}

/// What the fleet said, plus how many hosts were named -- everything a
/// per-element lookup needs, in one place so the argument count stays
/// inside the cap and the pieces cannot drift apart.
struct Fleet<'a> {
    seen: &'a Seen,
    served: Vec<&'a str>,
    named: usize,
}

impl<'a> Fleet<'a> {
    /// Borrow a probe result as a lookup context.
    fn of(seen: &'a Seen, named: usize) -> Self {
        Self {
            seen,
            served: seen.windows.keys().map(String::as_str).collect(),
            named,
        }
    }

    /// Every element's row, or the FIRST reason one could not be resolved.
    /// All-or-nothing: a partial list would read as a whole one (V44).
    fn rows_for(&self, names: &[&str], tokens: u64) -> Result<String, String> {
        names
            .iter()
            .map(|w| self.row_for(w, tokens))
            .collect::<Result<Vec<_>, _>>()
            .map(|rows| rows.concat())
    }

    /// One element's fit row, or the reason it could not be resolved.
    fn row_for(&self, want: &str, tokens: u64) -> Result<String, String> {
        match pick::resolve(&self.served, want) {
            Resolved::One(m) => {
                let w = self.seen.windows.get(m).copied().flatten();
                Ok(row(m, tokens, w) + "\n")
            }
            Resolved::Ambiguous(c) => Err(pick::ambiguous_msg(want, &c)),
            Resolved::Missing => Err(pick::missing_msg(
                want,
                &self.served,
                self.seen.answered,
                self.named,
            )),
        }
    }
}

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

/// model -> window across the fleet, merged in FLEET ORDER.
///
/// Probes every host CONCURRENTLY -- one thread each, no async runtime
/// (V23), and no pool because the host list is typed by hand (V24). A
/// dead host cost +3.05s serially on a three-host fleet; in parallel it
/// hides behind the live ones.
///
/// MERGED IN FLEET ORDER, which is the load-bearing half. `first host
/// that answers wins` would become `first host to REPLY wins` under
/// parallelism, so two runs could report different windows for a model
/// two hosts both serve -- a report-only verb contradicting itself (V5).
/// Joining in list order keeps precedence exactly where it was; only the
/// waiting overlaps.
fn fleet_windows(fleet: &[String]) -> Seen {
    merge(probe_all(fleet))
}

/// What one host serves: each model with its window, or `None` for the
/// window when `/api/show` did not answer for it.
type Served = Vec<(String, Option<u64>)>;

/// Probe all hosts at once; the result is per-host, in FLEET ORDER.
///
/// `None` means the host did not answer -- including a panicked probe
/// thread, which is treated as an unreachable host rather than taking the
/// process down with it.
fn probe_all(fleet: &[String]) -> Vec<Option<Served>> {
    let running: Vec<_> = fleet
        .iter()
        .cloned()
        .map(|base| std::thread::spawn(move || probe(&base)))
        .collect();
    running
        .into_iter()
        .map(|h| h.join().unwrap_or(None))
        .collect()
}

/// One host's models and their windows.
///
/// Every model this host serves is probed, even one another host already
/// covered. That is a deliberate cost: per-host INDEPENDENCE is what
/// makes hosts parallelizable at all, and the old lazy skip needed a
/// shared map the threads would have had to contend for (V89 -- no shared
/// mutable state, so no race to reason about).
fn probe(base: &str) -> Option<Served> {
    let models = ollama::models(base).ok()?;
    Some(
        models
            .into_iter()
            .map(|m| {
                let w = ollama::window(base, &m).ok();
                (m, w)
            })
            .collect(),
    )
}

/// What the fleet actually told us, and how much of the fleet told us.
/// The count travels WITH the data so a caller cannot report the union
/// without being able to say how complete it is (B11b).
#[derive(Default)]
struct Seen {
    windows: BTreeMap<String, Option<u64>>,
    answered: usize,
}

/// Fold per-host results into the union, first host in the LIST winning.
///
/// Pure, so the precedence rule is testable without a network: this is
/// the function parallelism could silently break (V5).
fn merge(probed: Vec<Option<Served>>) -> Seen {
    let mut seen = Seen::default();
    for host in probed {
        let Some(models) = host else {
            continue; // down host: skipped, and COUNTED (V44)
        };
        seen.answered = seen.answered.saturating_add(1);
        for (model, window) in models {
            seen.windows.entry(model).or_insert(window);
        }
    }
    seen
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

    /// V5, the property parallelism could silently break: the FIRST host
    /// in the LIST wins, not the first to reply.
    ///
    /// Two hosts serving the same model with different windows is the case
    /// that matters -- under completion-order merging, two runs of the same
    /// command could disagree, and a report-only verb that contradicts
    /// itself is broken. Pure, so it needs no network and cannot go flaky.
    #[test]
    fn the_first_host_in_the_list_wins() {
        let a = vec![("shared".to_owned(), Some(100u64))];
        let b = vec![("shared".to_owned(), Some(200u64))];
        let fwd = merge(vec![Some(a.clone()), Some(b.clone())]);
        assert_eq!(fwd.windows.get("shared"), Some(&Some(100)));
        // Reversing the FLEET reverses the answer -- proving the order is
        // the list's, not an accident of which value is larger or first
        // seen in some map.
        let rev = merge(vec![Some(b), Some(a)]);
        assert_eq!(rev.windows.get("shared"), Some(&Some(200)));
    }

    /// The union spans hosts, and `answered` counts only those that spoke
    /// (B11b/V44).
    #[test]
    fn the_union_spans_hosts_and_counts_the_silent_ones() {
        let seen = merge(vec![
            Some(vec![("a".to_owned(), Some(1))]),
            None, // a host that did not answer
            Some(vec![("b".to_owned(), Some(2))]),
        ]);
        assert_eq!(seen.windows.len(), 2, "union across the live hosts");
        assert_eq!(seen.answered, 2, "the silent host is not counted");
    }

    /// A host that answered but served nothing is still ANSWERED -- an
    /// empty fleet member is not a dead one (V47's distinction).
    #[test]
    fn an_empty_host_still_counts_as_answered() {
        let seen = merge(vec![Some(Vec::new())]);
        assert!(seen.windows.is_empty());
        assert_eq!(seen.answered, 1);
    }

    /// A model whose window did not resolve keeps its `None` rather than
    /// being replaced by a later host's -- precedence is by HOST, not by
    /// which host happened to have a usable answer. Deliberate: mixing
    /// them would make the reported window depend on a second host's
    /// health, which is the non-determinism this whole ordering prevents.
    #[test]
    fn a_hosts_unresolved_window_still_takes_precedence() {
        let seen = merge(vec![
            Some(vec![("m".to_owned(), None)]),
            Some(vec![("m".to_owned(), Some(9))]),
        ]);
        assert_eq!(seen.windows.get("m"), Some(&None));
    }

    /// T85: a comma-list names several models. One element that cannot be
    /// resolved fails the WHOLE call -- reporting the ones that worked while
    /// staying quiet about one that did not is B11b's shape, and the rule is
    /// applied here before shipping rather than after (V44).
    #[test]
    fn a_list_is_all_or_nothing() {
        let seen = seen_two();
        let fl = Fleet::of(&seen, 1);
        assert!(fl.row_for("gpt-oss", 1_000).is_ok(), "a resolvable element");
        let err = fl.row_for("llama", 1_000).err().unwrap_or_default();
        assert!(err.contains("no fleet host serves 'llama'"), "{err}");
        assert!(err.contains("1/1 host(s)"), "coverage is stated: {err}");
        // And the LIST fails as a whole, not partially.
        let both = fl.rows_for(&["gpt-oss", "llama"], 1_000);
        assert!(both.is_err(), "one bad element fails the call");
    }

    /// A two-model fleet, answered by one host of one. Owned by the test
    /// module so both list tests share one shape.
    fn seen_two() -> Seen {
        Seen {
            answered: 1,
            windows: [
                ("gpt-oss:20b".to_owned(), Some(131_072u64)),
                ("codestral:latest".to_owned(), Some(393_216u64)),
            ]
            .into(),
        }
    }

    /// The listing carries the shared denominator, and says so when the
    /// union behind it is partial (B11b/V44).
    #[test]
    fn the_listing_headers_the_shared_context_and_flags_a_partial_union() {
        let seen = seen_two();
        let whole = listing(1_000, "  row\n", &seen, 1);
        assert!(whole.starts_with("context: 1000 itok\n"));
        assert!(whole.contains("  row"));
        assert!(!whole.contains("PARTIAL"), "1 of 1 answered");
        let partial = listing(1_000, "  row\n", &seen, 3);
        assert!(partial.contains("1/3 host(s) answered"), "{partial}");
    }

    /// Each element resolves by V6 in its own right -- prefixes included,
    /// so a list is not a second, stricter grammar.
    #[test]
    fn every_element_resolves_by_the_same_rule() {
        let seen = seen_two();
        let fl = Fleet::of(&seen, 1);
        for want in ["gpt-oss", "gpt-oss:20b", "codestral"] {
            assert!(
                fl.row_for(want, 1).is_ok(),
                "{want} must resolve as it does alone"
            );
        }
    }

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
