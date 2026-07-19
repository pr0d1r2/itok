//! The `doctor` command: advisory pre-flight (V17). "Is this context
//! healthy to hand a model?" Composes itok-NATIVE signals only --
//! fit-to-window, balance (one file dominating), noise (lockfile share),
//! and confidence (the dummy-vs-bpe spread, the one signal only itok has,
//! because it owns both estimators). Report-only, exit 0 always: doctor
//! advises, `check` gates. A thin composer; it grows no tentacles.

use crate::args::{parse, Format, Opts};
use crate::cli::{Output, USAGE};
use crate::estimate::dummy;
use crate::render::Method;
use crate::walk::{bytes, tracked};
use std::path::{Path, PathBuf};

pub(crate) fn doctor(rest: &[String]) -> Output {
    match parse(rest) {
        Ok(opts) => run(&opts),
        Err(e) => Output::usage(format!("itok: {e}\n{USAGE}")),
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
            Output::ok(assess(opts, &root, model.encoding, window))
        }
        Err(e) => Output::usage(format!("itok: {e}\n{USAGE}")),
    }
}

/// Aggregate health of a fileset. Percentages are whole numbers.
struct Health {
    dummy_total: u64,
    real_total: u64,
    has_bpe: bool,
    method: &'static Method,
    window: Option<u64>,
    biggest: u64,
    noise: u64,
}

fn assess(
    opts: &Opts,
    root: &Path,
    method: &'static Method,
    window: Option<u64>,
) -> String {
    let files = if opts.paths.is_empty() {
        tracked(root)
    } else {
        opts.paths.clone()
    };
    let h = health(&files, root, window, method);
    match opts.format {
        Format::Json => json(&h),
        Format::Human => human(&h),
    }
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

/// Percentage `part` is of `whole`, saturating and division-safe.
fn pct(part: u64, whole: u64) -> u64 {
    part.saturating_mul(100).checked_div(whole).unwrap_or(0)
}

/// The dummy-vs-real spread: percent, and the direction bytes/4 erred.
fn spread(dummy_total: u64, real_total: u64) -> (u64, char) {
    if real_total >= dummy_total {
        (pct(real_total.saturating_sub(dummy_total), real_total), '+')
    } else {
        (
            pct(dummy_total.saturating_sub(real_total), dummy_total),
            '-',
        )
    }
}

/// Lockfiles are high-token, low-signal -- the noise a context carries.
fn is_noise(path: &str) -> bool {
    path.ends_with(".lock")
}

fn verdict(pct: u64, warn_at: u64) -> &'static str {
    if pct > 100 {
        "OVER"
    } else if pct >= warn_at {
        "warn"
    } else {
        "ok"
    }
}

fn human(h: &Health) -> String {
    let method = h.method.label;
    let mut s = format!("context: {} itok ({method})\n", h.real_total);
    s.push_str(&fit_line(h));
    s.push_str(&balance_line(h));
    s.push_str(&noise_line(h));
    s.push_str(&confidence_line(h));
    s
}

fn fit_line(h: &Health) -> String {
    match h.window {
        None => "  fit         (pass --window to check)\n".to_owned(),
        Some(w) => {
            let p = pct(h.real_total, w);
            format!(
                "  fit         {} / {w}  {p}%  {}\n",
                h.real_total,
                verdict(p, 80)
            )
        }
    }
}

fn balance_line(h: &Health) -> String {
    let p = pct(h.biggest, h.real_total);
    format!("  balance     biggest file {p}%  {}\n", verdict(p, 50))
}

fn noise_line(h: &Health) -> String {
    let p = pct(h.noise, h.real_total);
    format!("  noise       lockfiles {p}%  {}\n", verdict(p, 20))
}

fn confidence_line(h: &Health) -> String {
    if !h.has_bpe {
        return "  confidence  n/a (build with the bpe feature)\n".to_owned();
    }
    let (s, dir) = spread(h.dummy_total, h.real_total);
    format!("  confidence  bytes/4 is {dir}{s}% vs o200k\n")
}

fn json(h: &Health) -> String {
    let method = h.method.label;
    let window = h
        .window
        .map_or_else(|| "null".to_owned(), |w| w.to_string());
    let fit = h.window.map_or(0, |w| pct(h.real_total, w));
    format!(
        "{{\"total_tokens\":{},\"method\":\"{method}\",\"window\":{window},\
         \"fit_pct\":{fit},\"biggest_pct\":{},\"noise_pct\":{},\
         \"confidence_pct\":{}}}\n",
        h.real_total,
        pct(h.biggest, h.real_total),
        pct(h.noise, h.real_total),
        spread(h.dummy_total, h.real_total).0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIR: &str = env!("CARGO_MANIFEST_DIR");

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn pct_is_division_safe() {
        assert_eq!(pct(50, 200), 25);
        assert_eq!(pct(5, 0), 0);
    }

    #[test]
    fn spread_reports_direction() {
        assert_eq!(spread(80, 100), (20, '+'));
        assert_eq!(spread(100, 80), (20, '-'));
    }

    #[test]
    fn lockfiles_are_noise() {
        assert!(is_noise("Cargo.lock"));
        assert!(!is_noise("src/main.rs"));
    }

    #[test]
    fn verdict_thresholds() {
        assert_eq!(verdict(10, 80), "ok");
        assert_eq!(verdict(90, 80), "warn");
        assert_eq!(verdict(120, 80), "OVER");
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

    // The has_bpe:false and window:None branches are unreachable through a
    // default (bpe-on, real-repo) run, so drive them with a built Health.
    fn h(real: u64, dummy: u64, window: Option<u64>, bpe: bool) -> Health {
        let method = if bpe {
            &crate::render::O200K
        } else {
            &crate::render::DUMMY
        };
        Health {
            dummy_total: dummy,
            real_total: real,
            has_bpe: bpe,
            method,
            window,
            biggest: real,
            noise: 0,
        }
    }

    #[test]
    fn confidence_is_na_without_bpe() {
        assert!(confidence_line(&h(100, 80, None, false)).contains("n/a"));
    }

    #[test]
    fn human_names_the_dummy_method_without_bpe() {
        assert!(human(&h(100, 80, None, false)).contains("bytes/4"));
    }

    #[test]
    fn json_window_is_null_when_absent() {
        let j = json(&h(100, 80, None, true));
        assert!(j.contains("\"window\":null"));
        assert!(j.contains("\"fit_pct\":0"));
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
