//! The `estimate` command: Opts -> Output. Split from cli.rs at the byte
//! ceiling (V483) -- cli.rs is the verb-dispatch spine, and each verb's
//! orchestration lives in its own module. The next verbs follow this
//! shape.

use crate::args::{parse, Format, Opts};
use crate::cli::Output;
use crate::estimate::{measure, over_budget, Estimate};
use crate::json;
#[cfg(feature = "bpe")]
use crate::render::O200K;
use crate::render::{report, Method, Style, DUMMY};
#[cfg(feature = "ollama")]
use std::path::Path;
use std::path::PathBuf;

pub(crate) fn estimate(rest: &[String]) -> Output {
    match parse(rest) {
        Ok(opts) => graded(&opts),
        Err(e) => Output::usage_err(format!("itok: {e}")),
    }
}

/// Estimate, render, and apply `--budget`: the report always prints; a
/// breach adds a stderr note and exit 1 (V16). Without a budget it is
/// report-only, exit 0 (V5). `--ollama` is the exact tier: network and
/// fallible, so a host/model failure is exit 7 (V22/V23).
fn graded(opts: &Opts) -> Output {
    let root = PathBuf::from(opts.chdir.as_deref().unwrap_or("."));
    #[cfg(feature = "ollama")]
    if opts.is_ollama() {
        return exact(opts, &root);
    }
    match measure(opts, &root) {
        Ok(ests) => present(&ests, opts, method(opts)),
        Err(e) => Output::usage_err(format!("itok: {e}")),
    }
}

/// The exact tier's run, with its failure carrying the exit code it earns.
#[cfg(feature = "ollama")]
fn exact(opts: &Opts, root: &Path) -> Output {
    match exact_measure(opts, root) {
        Ok((ests, m)) => present(&ests, opts, &m),
        Err(f) => Output {
            out: String::new(),
            err: format!("itok: {}\n", f.msg),
            code: f.code,
        },
    }
}

/// Render the estimates and apply `--budget`; shared by every tier.
fn present(ests: &[Estimate], opts: &Opts, m: &Method) -> Output {
    let out = rendered(ests, opts, m);
    let breaches = over_budget(ests, opts.budget);
    if breaches.is_empty() {
        Output::ok(out)
    } else {
        Output::breach(out, breach_msg(&breaches, opts.budget.unwrap_or(0)))
    }
}

/// The exact tier (V22): each file's TRUE count from a local model's own
/// tokenizer via ollama. Fallible -- a missing host/model aborts the whole
/// run with exit 7 rather than a partial or estimated number.
/// An exact-tier failure and the exit code it EARNS.
///
/// A network failure is 7; a bad `--model` is 2. Everything used to be 7,
/// which made a typo indistinguishable from an unreachable host -- so a CI
/// retry loop would spin on the typo. `From<String>` keeps 7 as the
/// default, so existing `?` sites are unchanged and only the argument path
/// opts into 2 (the exit table's own split).
#[cfg(feature = "ollama")]
struct Fail {
    msg: String,
    code: u8,
}

#[cfg(feature = "ollama")]
impl From<String> for Fail {
    fn from(msg: String) -> Self {
        Self { msg, code: 7 }
    }
}

#[cfg(feature = "ollama")]
fn exact_measure(
    opts: &Opts,
    root: &Path,
) -> Result<(Vec<Estimate>, Method), Fail> {
    // Counting needs ONE host+model; a fleet's union is the doctor concept
    // (V24). Use the first resolved host.
    let fleet =
        crate::ollama::hosts::bases(opts.ollama_hosts.as_deref(), root)?;
    let base = fleet.first().ok_or_else(|| "no ollama host".to_owned())?;
    let model = pick_model(opts, base)?;
    let mut ests = Vec::new();
    // A directory / unreadable path is the USER's error, not the
    // network's, so it keeps exit 2 rather than inheriting 7 (B11d).
    let files = crate::estimate::select(opts, root)
        .map_err(|msg| Fail { msg, code: 2 })?;
    for f in files {
        let text = std::fs::read_to_string(root.join(&f))
            .map_err(|e| format!("{f}: {e}"))?;
        let tokens = crate::ollama::count(base, &model, &text)?;
        ests.push(Estimate { path: f, tokens });
    }
    // The label names the endpoint that produced these counts, so a
    // number from an unintended tokenizer is visible (V101/V3).
    let m = crate::render::exact_via(&model, base);
    Ok((crate::estimate::cap(ests, opts.top), m))
}

/// The model to tokenize with: `--model` if given, else the host's first
/// served model (the common single-model case).
#[cfg(feature = "ollama")]
fn pick_model(opts: &Opts, base: &str) -> Result<String, Fail> {
    let served = crate::ollama::models(base)?;
    let names: Vec<&str> = served.iter().map(String::as_str).collect();
    let Some(want) = opts.model.as_deref() else {
        return names
            .first()
            .map(|m| (*m).to_owned())
            .ok_or_else(|| "ollama host serves no models".to_owned().into());
    };
    resolved(want, &names)
}

/// The SAME resolution `doctor` uses (V6), reached through the shared
/// module rather than reimplemented -- B11a was the second copy's absence,
/// not its divergence: `estimate --ollama --model gpt-oss` posted the
/// literal name and 404'd while `doctor` resolved it (V64).
#[cfg(feature = "ollama")]
fn resolved(want: &str, names: &[&str]) -> Result<String, Fail> {
    use crate::ollama::pick::{self, Resolved};
    match pick::resolve(names, want) {
        Resolved::One(m) => Ok(m.to_owned()),
        Resolved::Ambiguous(c) => Err(Fail {
            msg: pick::ambiguous_msg(want, &c),
            code: 2,
        }),
        // One host here (counting needs one tokenizer), so answered = 1.
        Resolved::Missing => Err(Fail {
            msg: pick::missing_msg(want, names, 1, 1),
            code: 2,
        }),
    }
}

fn breach_msg(breaches: &[&Estimate], budget: u64) -> String {
    let mut s =
        format!("itok: {} file(s) over budget {budget}:\n", breaches.len());
    for e in breaches {
        s.push_str(&format!("  {} itok  {}\n", e.tokens, e.path));
    }
    s
}

/// The LOCAL tiers' method -- each one IS its name (V3).
///
/// The remote tier is absent here on purpose: `exact` is incomplete
/// without the tokenizer that produced it, and that is only known after
/// the host and model resolve, so `exact_measure` builds its own (V101).
fn method(opts: &Opts) -> &'static Method {
    match opts.tier {
        #[cfg(feature = "bpe")]
        crate::args::Tier::Bpe => &O200K,
        #[cfg(feature = "ollama")]
        crate::args::Tier::Ollama => &DUMMY, // unreachable: see `exact`
        crate::args::Tier::Dummy => &DUMMY,
    }
}

/// Human table (cosmetic) or JSONL (stable contract), per `--format`.
fn rendered(ests: &[Estimate], opts: &Opts, m: &Method) -> String {
    match opts.format {
        Format::Json => json::report(ests, m),
        Format::Human => {
            let style = Style {
                summarize: opts.summarize,
                human: opts.human,
            };
            report(ests, &style, m)
        }
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
    fn names_unit_and_method() {
        let o = estimate(&args(&["-C", DIR, "Cargo.toml"]));
        assert!(o.out.contains("Cargo.toml"));
        assert!(o.out.contains("itok"));
        assert!(o.out.contains("total (bytes/4)"));
        assert_eq!(o.code, 0);
    }

    #[test]
    fn summarize_hides_the_per_file_lines() {
        let o = estimate(&args(&["-s", "-C", DIR, "Cargo.toml"]));
        assert!(!o.out.contains("Cargo.toml"));
        assert!(o.out.contains("total"));
    }

    #[test]
    fn human_abbreviates_the_count() {
        let o = estimate(&args(&["-h", "-C", DIR, "SPEC.md"]));
        assert!(o.out.contains('k'));
    }

    #[test]
    fn json_carries_the_stable_schema() {
        let o = estimate(&args(&["--format", "json", "-C", DIR, "Cargo.toml"]));
        assert!(o.out.contains("\"unit\":\"input_tokens\""));
        assert!(o.out.contains("\"estimated\":true"));
    }

    #[test]
    fn json_is_one_object_per_file() {
        let o = estimate(&args(&[
            "--format",
            "json",
            "-C",
            DIR,
            "Cargo.toml",
            "SPEC.md",
        ]));
        assert_eq!(o.out.lines().count(), 2);
    }

    #[test]
    fn an_unknown_format_is_a_usage_error() {
        assert_eq!(estimate(&args(&["--format", "yaml"])).code, 2);
    }

    #[test]
    fn top_without_a_number_is_a_usage_error() {
        assert_eq!(estimate(&args(&["--top"])).code, 2);
    }

    #[test]
    fn no_paths_uses_the_tracked_set() {
        let o = estimate(&args(&["-C", DIR]));
        assert!(o.out.contains("total"));
        assert_eq!(o.code, 0);
    }

    #[test]
    fn over_budget_exits_one_and_still_reports() {
        let o = estimate(&args(&["--budget", "1k", "-C", DIR, "SPEC.md"]));
        assert_eq!(o.code, 1);
        assert!(o.err.contains("over budget"));
        assert!(o.err.contains("SPEC.md"));
        assert!(o.out.contains("itok"), "report still prints");
    }

    #[test]
    fn under_budget_exits_zero() {
        let o = estimate(&args(&["--budget", "1M", "-C", DIR, "Cargo.toml"]));
        assert_eq!(o.code, 0);
        assert!(o.err.is_empty());
    }

    #[cfg(feature = "bpe")]
    #[test]
    fn bpe_gives_a_true_count_without_a_tilde() {
        let o = estimate(&args(&["--bpe", "-C", DIR, "Cargo.toml"]));
        assert_eq!(o.code, 0);
        assert!(o.out.contains("o200k"));
        assert!(!o.out.contains('~'), "true count, no tilde: {:?}", o.out);
    }

    #[cfg(feature = "bpe")]
    #[test]
    fn bpe_json_is_not_estimated() {
        let o = estimate(&args(&[
            "--bpe",
            "--format",
            "json",
            "-C",
            DIR,
            "Cargo.toml",
        ]));
        assert!(o.out.contains("\"estimated\":false"));
        assert!(o.out.contains("\"method\":\"o200k\""));
    }
}
