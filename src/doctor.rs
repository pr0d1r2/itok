//! The `doctor` command: advisory pre-flight (V17). "Is this context
//! healthy to hand a model?" Composes itok-NATIVE signals only --
//! fit-to-window, balance (one file dominating), noise (lockfile share),
//! and confidence (the dummy-vs-bpe spread, the one signal only itok has,
//! because it owns both estimators). Report-only, exit 0 always: doctor
//! advises, `check` gates. A thin composer; it grows no tentacles. This
//! module gathers the signals; `doctorfmt` renders them.

use crate::args::{Format, Opts, parse};
use crate::cli::Output;
use crate::doctorfmt::{Health, human, json};
use crate::estimate::dummy;
use crate::render::Method;
use crate::walk::bytes;
use std::path::{Path, PathBuf};

pub(crate) fn doctor(rest: &[String]) -> Output {
    // `--session` retargets the verb at a CONTEXT (V99/T76). Routed
    // BEFORE the shared parser, because the flag belongs to doctor and
    // putting it in `Opts` would hand it to `estimate` and `fit` too.
    #[cfg(feature = "session")]
    if crate::doctorsession::targeted(rest) {
        return crate::doctorsession::session(rest);
    }
    match parse(rest) {
        Ok(opts) => run(&opts),
        Err(e) => Output::usage_err(format!("itok: {e}")),
    }
}

fn run(opts: &Opts) -> Output {
    let root = PathBuf::from(opts.chdir.as_deref().unwrap_or("."));
    // `--ollama` discovers live windows from the endpoint (V22).
    #[cfg(feature = "ollama")]
    if opts.is_ollama() {
        return crate::discover::run(opts, &root);
    }
    // `--model` resolves encoding + window from `.context-models` (V11/V18);
    // explicit `--window` wins over the table.
    match crate::models::tier(opts.model.as_deref(), &root) {
        Ok(model) => {
            let window = opts.window.or(model.window);
            match assess(opts, &root, model.encoding, window) {
                Ok(out) => Output::ok(out),
                Err(e) => Output::usage_err(format!("itok: {e}")),
            }
        }
        Err(e) => Output::usage_err(format!("itok: {e}")),
    }
}

fn assess(
    opts: &Opts,
    root: &Path,
    method: &'static Method,
    window: Option<u64>,
) -> Result<String, String> {
    // ONE selection rule, shared (V64): doctor used to carry its own copy,
    // which is how it kept B11d after estimate was fixed.
    let files = crate::estimate::select(opts, root)?;
    let h = health(&files, root, window, method);
    Ok(match opts.format {
        Format::Json => json(&h),
        Format::Human => human(&h),
    })
}

fn health(
    files: &[String],
    root: &Path,
    window: Option<u64>,
    method: &'static Method,
) -> Health {
    let (dummy_total, real_total, biggest, noise) = totals(files, root);
    Health {
        dummy_total,
        real_total,
        has_bpe: cfg!(feature = "bpe"),
        method,
        window,
        biggest,
        noise,
    }
}

/// (dummy_total, real_total, biggest_file, noise_tokens) over the fileset.
fn totals(files: &[String], root: &Path) -> (u64, u64, u64, u64) {
    let (mut d, mut r, mut big, mut noise) = (0u64, 0u64, 0u64, 0u64);
    for f in files {
        let Some(len) = bytes(&root.join(f)) else {
            continue;
        };
        let fd = dummy(len);
        let fr = real_count(&root.join(f), fd);
        d = d.saturating_add(fd);
        r = r.saturating_add(fr);
        big = big.max(fr);
        if is_noise(f) {
            noise = noise.saturating_add(fr);
        }
    }
    (d, r, big, noise)
}

/// The real-tier count for a file: bpe for text, dummy for a binary
/// (which the tokenizer cannot read) or when the feature is off.
fn real_count(path: &Path, dummy_val: u64) -> u64 {
    #[cfg(feature = "bpe")]
    {
        std::fs::read_to_string(path)
            .ok()
            .map_or(dummy_val, |t| crate::bpe::count(&t))
    }
    #[cfg(not(feature = "bpe"))]
    {
        let _ = path;
        dummy_val
    }
}

/// Lockfiles are high-token, low-signal -- the noise a context carries.
fn is_noise(path: &str) -> bool {
    path.ends_with(".lock")
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIR: &str = env!("CARGO_MANIFEST_DIR");

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn lockfiles_are_noise() {
        assert!(is_noise("Cargo.lock"));
        assert!(!is_noise("src/main.rs"));
    }

    #[test]
    fn doctor_is_advisory_exit_zero() {
        let o = doctor(&args(&["-C", DIR, "SPEC.md"]));
        assert_eq!(o.code, 0);
        assert!(o.out.contains("context:"));
        assert!(o.out.contains("balance"));
        assert!(o.out.contains("confidence"));
    }

    #[test]
    fn window_gives_a_fit_percentage() {
        let o = doctor(&args(&["--window", "1M", "-C", DIR, "SPEC.md"]));
        assert!(o.out.contains("fit"));
        assert!(o.out.contains('%'));
    }

    #[test]
    fn without_window_fit_asks_for_one() {
        let o = doctor(&args(&["-C", DIR, "Cargo.toml"]));
        assert!(o.out.contains("pass --window"));
    }

    #[test]
    fn json_carries_the_signals() {
        let o = doctor(&args(&[
            "--format", "json", "--window", "1M", "-C", DIR, "SPEC.md",
        ]));
        assert!(o.out.contains("\"total_tokens\":"));
        assert!(o.out.contains("\"fit_pct\":"));
        assert!(o.out.contains("\"confidence_pct\":"));
    }

    #[test]
    fn a_bad_flag_is_a_usage_error() {
        assert_eq!(doctor(&args(&["--bogus"])).code, 2);
    }

    #[test]
    fn no_paths_over_the_repo_reports_noise() {
        // Root scope (no path arg) exercises the tracked-set branch. Repo
        // root from git (V37) so it holds in the extraction rehearsal (T11).
        let root = crate::testutil::repo_root();
        let o = doctor(&args(&["-C", &root]));
        assert_eq!(o.code, 0);
        assert!(o.out.contains("noise"));
    }

    #[cfg(feature = "bpe")]
    #[test]
    fn an_unreadable_path_falls_back_to_dummy() {
        // read_to_string fails on a directory; real_count returns dummy.
        assert_eq!(real_count(Path::new(DIR), 42), 42);
    }

    #[test]
    fn a_missing_file_is_skipped() {
        // A tracked path absent from disk yields no metadata -> skipped.
        assert_eq!(
            totals(&["no-such-itok-file".to_owned()], Path::new(DIR)),
            (0, 0, 0, 0)
        );
    }
}
