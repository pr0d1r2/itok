//! The `estimate` command: Opts -> Output. Split from cli.rs at the byte
//! ceiling (V483) -- cli.rs is the verb-dispatch spine, and each verb's
//! orchestration lives in its own module. The next verbs follow this
//! shape.

use crate::args::{parse, Format, Opts};
use crate::cli::{Output, USAGE};
use crate::estimate::{measure, over_budget, Estimate};
use crate::json;
#[cfg(feature = "ollama")]
use crate::render::EXACT;
#[cfg(feature = "bpe")]
use crate::render::O200K;
use crate::render::{report, Method, Style, DUMMY};
#[cfg(feature = "ollama")]
use std::path::Path;
use std::path::PathBuf;

pub(crate) fn estimate(rest: &[String]) -> Output {
    match parse(rest) {
        Ok(opts) => graded(&opts),
        Err(e) => Output::usage(format!("itok: {e}\n{USAGE}")),
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
        return match exact_measure(opts, &root) {
            Ok(ests) => present(&ests, opts),
            Err(e) => Output {
                out: String::new(),
                err: format!("itok: {e}\n"),
                code: 7,
            },
        };
    }
    present(&measure(opts, &root), opts)
}

/// Render the estimates and apply `--budget`; shared by every tier.
fn present(ests: &[Estimate], opts: &Opts) -> Output {
    let out = rendered(ests, opts);
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
#[cfg(feature = "ollama")]
fn exact_measure(opts: &Opts, root: &Path) -> Result<Vec<Estimate>, String> {
    // Counting needs ONE host+model; a fleet's union is the doctor concept
    // (V24). Use the first resolved host.
    let fleet =
        crate::ollama::hosts::bases(opts.ollama_hosts.as_deref(), root)?;
    let base = fleet.first().ok_or_else(|| "no ollama host".to_owned())?;
    let model = pick_model(opts, base)?;
    let mut ests = Vec::new();
    for f in crate::estimate::select(opts, root) {
        let text = std::fs::read_to_string(root.join(&f))
            .map_err(|e| format!("{f}: {e}"))?;
        let tokens = crate::ollama::count(base, &model, &text)?;
        ests.push(Estimate { path: f, tokens });
    }
    Ok(crate::estimate::cap(ests, opts.top))
}

/// The model to tokenize with: `--model` if given, else the host's first
/// served model (the common single-model case).
#[cfg(feature = "ollama")]
fn pick_model(opts: &Opts, base: &str) -> Result<String, String> {
    match &opts.model {
        Some(m) => Ok(m.clone()),
        None => crate::ollama::models(base)?
            .into_iter()
            .next()
            .ok_or_else(|| "ollama host serves no models".to_owned()),
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

/// The method the numbers were reached by -- names the tier (V3). Exact
/// (ollama) beats bpe beats dummy.
fn method(opts: &Opts) -> &'static Method {
    match opts.tier {
        #[cfg(feature = "ollama")]
        crate::args::Tier::Ollama => &EXACT,
        #[cfg(feature = "bpe")]
        crate::args::Tier::Bpe => &O200K,
        crate::args::Tier::Dummy => &DUMMY,
    }
}

/// Human table (cosmetic) or JSONL (stable contract), per `--format`.
fn rendered(ests: &[Estimate], opts: &Opts) -> String {
    let m = method(opts);
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
