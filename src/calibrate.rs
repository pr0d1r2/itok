//! The `calibrate` command: what this session's context ACTUALLY costs,
//! against what itok estimated (V102).
//!
//! The runtime axis closes the estimate-vs-truth loop for free, because the
//! transcript carries the real `usage` (V43). This verb reports the two
//! parameters that relate them: a FIXED overhead the transcript cannot see
//! -- system prompt plus tool schemas -- and a SCALE from `bytes/4` of
//! transcript content to actual billed tokens.
//!
//! SUPERSEDES V48's method, not its rules. V48 derived one factor from
//! turns where exactly ONE load explained the delta, discarding the rest; a
//! two-parameter fit needs no per-item attribution at all, so it uses every
//! turn and yields the overhead as a second output. What survives unchanged
//! is the honesty: the factor is REPORTED, never folded silently into the
//! estimator ladder (V4/V48), and applying it stays the caller's choice.
//!
//! Reports the BAND and `n`, never a bare point (V102). The band comes from
//! turns the fit never saw -- in-sample residuals understate, and an
//! understated band is precisely what makes a fit verdict dishonest.
//!
//! Report-only, exit 0 (V5). Too few turns reports `n` and NO factor, the
//! same rule as a missing denominator (V92): a fit from three points is a
//! fiction that looks measured.

use crate::args::Format;
use crate::cli::Output;
use crate::render::human;
use crate::session::{Fit, Session, MIN_FIT_TURNS};
use crate::tracecmd::value;

#[derive(Default)]
struct Raw {
    session: Option<String>,
    human: bool,
    format: Format,
    chdir: Option<String>,
}

pub(crate) fn calibrate(rest: &[String]) -> Output {
    match parse(rest) {
        Ok(raw) => run(&raw),
        Err(e) => Output::usage_err(format!("itok: {e}")),
    }
}

fn run(raw: &Raw) -> Output {
    // An INFERRED absence is not an error; a NAMED miss is (V104).
    let parsed = match read(raw) {
        Ok(p) => p,
        Err(o) => return o,
    };
    Output::ok(match raw.format {
        Format::Json => json(&parsed),
        Format::Human => report(&parsed, raw.human),
    })
}

fn read(raw: &Raw) -> Result<Session, Output> {
    crate::tracecmd::session_at(raw.session.as_deref(), raw.chdir.as_deref())
        .map(|(parsed, _)| parsed)
}

/// Turns that carried a usable window -- the fit's sample size, and the
/// number a reader needs to weigh everything else (V48's `n`).
fn usable(parsed: &Session) -> usize {
    parsed
        .turns
        .iter()
        .filter(|t| t.billed_input().is_some_and(|w| w > 0))
        .count()
}

fn report(parsed: &Session, h: bool) -> String {
    let n = usable(parsed);
    let Some(f) = parsed.fit() else {
        return too_few(n);
    };
    params(&f, h) + &measured(&f, parsed, h) + CAVEAT
}

/// The two fitted parameters and the band they carry.
fn params(f: &Fit, h: bool) -> String {
    format!(
        "{:>9} itok  overhead   (fixed: system prompt + tool schemas)\n\
         {:>9}      scale      (bytes/4 -> actual, fitted n={})\n\
         {:>9}      band       (max held-out error, validated n={})\n",
        size(f.overhead, h),
        scale_str(f.scale_milli),
        f.fitted,
        band_str(f.band_permille),
        f.validated,
    )
}

/// The loop closed: what was billed, beside what the fit predicts.
fn measured(f: &Fit, parsed: &Session, h: bool) -> String {
    format!(
        "{:>9} itok  window     (actual, last turn)\n\
         {:>9} itok  predicted  (overhead + scale x bytes/4)\n",
        size(parsed.window().unwrap_or(0), h),
        format!("~{}", size(f.predict(parsed.content_bytes), h)),
    )
}

/// Says `n` and stops.
///
/// V92's rule, on precision instead of a denominator: without enough turns
/// there is no honest band, and a factor without a band is the point
/// estimate V102 forbids.
fn too_few(n: usize) -> String {
    format!(
        "  n={n} turns -- too few to fit (need {MIN_FIT_TURNS}); \
         no factor reported\n"
    )
}

/// What the numbers rest on.
///
/// The scale is NOT a tokenizer ratio, and saying so is load-bearing: it
/// also absorbs per-message framing and the `thinking` text the transcript
/// stores empty. Naming that keeps a reader from carrying the number to
/// another harness where it does not hold (V102/V3).
const CAVEAT: &str = "  scale absorbs message framing and unrecorded \
     reasoning -- not a tokenizer ratio; derived from THIS session\n";

fn scale_str(milli: u64) -> String {
    let whole = milli.checked_div(1000).unwrap_or(0);
    let frac = milli.checked_rem(1000).unwrap_or(0);
    format!("{whole}.{frac:03}x")
}

fn band_str(permille: u64) -> String {
    let whole = permille.checked_div(10).unwrap_or(0);
    let frac = permille.checked_rem(10).unwrap_or(0);
    format!("+-{whole}.{frac}%")
}

fn size(n: u64, h: bool) -> String {
    if h {
        human(n)
    } else {
        n.to_string()
    }
}

/// One object (V9). `null` for every field when the sample cannot support
/// a fit -- keys stay so a parser learns one shape, and `null` is json's
/// own word for "not measured" (V47).
fn json(parsed: &Session) -> String {
    let n = usable(parsed);
    match parsed.fit() {
        Some(f) => json_fit(&f, parsed, n),
        None => format!(
            "{{\"turns\":{n},\"min_turns\":{MIN_FIT_TURNS},\
             \"overhead\":null,\"scale_milli\":null,\"band_permille\":null,\
             \"predicted\":null,{}}}\n",
            METHOD
        ),
    }
}

fn json_fit(f: &Fit, parsed: &Session, n: usize) -> String {
    format!(
        "{{\"turns\":{n},\"min_turns\":{MIN_FIT_TURNS},\
         \"overhead\":{},\"scale_milli\":{},\"band_permille\":{},\
         \"fitted\":{},\"validated\":{},\"window\":{},\"predicted\":{},{}}}\n",
        f.overhead,
        f.scale_milli,
        f.band_permille,
        f.fitted,
        f.validated,
        parsed.window().unwrap_or(0),
        f.predict(parsed.content_bytes),
        METHOD
    )
}

/// Named units, so a parser never has to guess what 1571 means (V3/V9).
const METHOD: &str = "\"unit\":\"input_tokens\",\
     \"scale_unit\":\"thousandths_per_bytes4_unit\",\
     \"band_unit\":\"tenths_of_percent\",\"window_method\":\"actual\",\
     \"predicted_method\":\"fitted overhead + scale x bytes/4\"";

fn parse(rest: &[String]) -> Result<Raw, String> {
    let mut raw = Raw::default();
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        apply(a, &mut it, &mut raw)?;
    }
    Ok(raw)
}

fn apply<'a>(
    a: &str,
    it: &mut impl Iterator<Item = &'a String>,
    raw: &mut Raw,
) -> Result<(), String> {
    match a {
        "-h" => raw.human = true,
        "--bpe" | "--ollama" => {
            return Err(crate::tracecmd::no_real_tier(a, "calibrate"))
        }
        "--format" => raw.format = crate::tracecmd::format_of(&value(it, a)?)?,
        "-C" => raw.chdir = Some(value(it, a)?),
        other if other.starts_with('-') => {
            return Err(format!("unknown flag {other}"))
        }
        other => raw.session = Some(other.to_owned()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{Turn, Verdict};

    fn fixture(name: &str) -> String {
        format!(
            "{}/tests/fixtures/session/{name}",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    /// A session whose windows follow `overhead + scale x content` EXACTLY.
    /// Synthetic on purpose: real data cannot tell you whether a fit
    /// recovered the truth, because the truth is unknown. Here it is known.
    fn exact_session(overhead: u64, scale_milli: u64, n: usize) -> Session {
        let mut s = Session::default();
        for i in 1..=n {
            let bytes = u64::try_from(i).unwrap_or(0).saturating_mul(4_000);
            s.turns.push(Turn {
                cache_read: Some(exact_window(overhead, scale_milli, bytes)),
                cum_content_bytes: bytes,
                ..Turn::default()
            });
            s.content_bytes = bytes;
        }
        s
    }

    /// The model the fit must recover, written out once.
    fn exact_window(overhead: u64, scale_milli: u64, bytes: u64) -> u64 {
        let units = bytes.checked_div(4).unwrap_or(0);
        overhead.saturating_add(
            units
                .saturating_mul(scale_milli)
                .checked_div(1000)
                .unwrap_or(0),
        )
    }

    /// V102, the test that matters: the fit RECOVERS known parameters.
    #[test]
    fn the_fit_recovers_known_parameters() {
        let s = exact_session(32_000, 1_500, 40);
        let f = s.fit().unwrap_or_default();
        assert_eq!(f.overhead, 32_000, "intercept");
        assert_eq!(f.scale_milli, 1_500, "slope");
        assert_eq!(f.band_permille, 0, "an exact model has no error");
    }

    /// V102: the band comes from turns the fit NEVER SAW, and both counts
    /// are reported so a reader can weigh them.
    #[test]
    fn the_band_is_measured_on_held_out_turns() {
        let s = exact_session(1_000, 2_000, 40);
        let f = s.fit().unwrap_or_default();
        assert_eq!(f.fitted, 20);
        assert_eq!(f.validated, 20);
        assert_eq!(f.fitted + f.validated, 40, "every turn is accounted for");
    }

    /// Noise in the held-out half shows up in the BAND rather than being
    /// smoothed away -- the band is the honest part of the report.
    #[test]
    fn held_out_error_widens_the_band() {
        let mut s = exact_session(1_000, 1_000, 40);
        if let Some(t) = s.turns.last_mut() {
            t.cache_read = t.cache_read.map(|w| w.saturating_mul(2));
        }
        let f = s.fit().unwrap_or_default();
        assert!(f.band_permille > 0, "a bad prediction must widen the band");
    }

    /// V92's shape: too few turns reports `n` and NO factor. A fit from
    /// three points is a fiction that looks measured.
    #[test]
    fn too_few_turns_reports_n_and_no_factor() {
        let s = exact_session(1_000, 1_000, MIN_FIT_TURNS - 1);
        assert_eq!(s.fit(), None);
        let out = report(&s, false);
        assert!(out.contains("too few to fit"));
        assert!(out.contains(&format!("n={}", MIN_FIT_TURNS - 1)));
        assert!(!out.contains("scale"), "no factor may appear");
    }

    /// A sample with no variation in content cannot determine a line.
    /// `None`, not an infinity that would render as a number.
    #[test]
    fn a_degenerate_sample_yields_no_fit() {
        let mut s = Session::default();
        for _ in 0..40 {
            s.turns.push(Turn {
                cache_read: Some(5_000),
                cum_content_bytes: 4_000, // identical every turn
                ..Turn::default()
            });
        }
        assert_eq!(s.fit(), None, "zero denominator: no line exists");
    }

    /// V93's rule, inherited: a zero-window turn is excluded rather than
    /// dragging the intercept toward a measurement that never happened.
    #[test]
    fn zero_window_turns_are_excluded_from_the_fit() {
        let mut s = exact_session(32_000, 1_500, 40);
        s.turns.push(Turn {
            cache_read: Some(0),
            cum_content_bytes: 999_999,
            ..Turn::default()
        });
        let f = s.fit().unwrap_or_default();
        assert_eq!(f.overhead, 32_000, "the zero turn did not move it");
    }

    /// V102's teeth: inside the band the fit REFUSES to answer. A 5% band
    /// on 340k is +-17k -- decisive at 20% full, worthless at 97%.
    #[test]
    fn a_verdict_inside_the_band_is_refused() {
        let f = Fit {
            overhead: 0,
            scale_milli: 1_000,
            fitted: 20,
            validated: 20,
            band_permille: 50, // +-5%
        };
        assert_eq!(f.verdict(100_000, 200_000), Verdict::Fits);
        assert_eq!(f.verdict(100_000, 50_000), Verdict::Over);
        // 100k +-5k against a 102k window: the answer is inside the band.
        assert_eq!(f.verdict(100_000, 102_000), Verdict::TooClose);
        assert_eq!(f.verdict(100_000, 98_000), Verdict::TooClose);
    }

    /// A zero band still refuses only where it must: an exact model can
    /// answer right up to the boundary.
    #[test]
    fn a_zero_band_answers_at_the_boundary() {
        let f = Fit {
            overhead: 0,
            scale_milli: 1_000,
            fitted: 20,
            validated: 20,
            band_permille: 0,
        };
        assert_eq!(f.verdict(100_000, 100_000), Verdict::Fits);
        assert_eq!(f.verdict(100_001, 100_000), Verdict::Over);
    }

    /// `predict` applies the fit to a content size -- the whole point of
    /// deriving it (V48: applying is opt-in, and this is the entry point).
    #[test]
    fn predict_applies_overhead_and_scale() {
        let f = Fit {
            overhead: 1_000,
            scale_milli: 1_500,
            fitted: 20,
            validated: 20,
            band_permille: 0,
        };
        // 4000 bytes = 1000 units; 1000 x 1.5 = 1500; + 1000 overhead.
        assert_eq!(f.predict(4_000), 2_500);
    }

    /// V3: the derived number carries the tilde, the measured one does not.
    #[test]
    fn predicted_is_marked_estimated_and_the_window_is_not() {
        let s = exact_session(32_000, 1_500, 40);
        let out = report(&s, false);
        let line = |k: &str| {
            out.lines()
                .find(|l| l.contains(k))
                .unwrap_or_default()
                .to_owned()
        };
        assert!(line("predicted").contains('~'), "derived: tilde");
        assert!(!line("window").contains('~'), "actual: no tilde");
        assert!(!line("overhead").contains('~'), "fitted from actuals");
    }

    /// V102/V3: the report says what the scale is NOT, because a reader who
    /// took it for a tokenizer ratio would carry it somewhere it fails.
    #[test]
    fn the_report_denies_being_a_tokenizer_ratio() {
        let out = report(&exact_session(32_000, 1_500, 40), false);
        assert!(out.contains("not a tokenizer ratio"));
        assert!(out.contains("THIS session"), "scoped to one session");
    }

    /// V59: arithmetic and method, never advice.
    #[test]
    fn the_report_gives_no_advice() {
        let out =
            report(&exact_session(32_000, 1_500, 40), false).to_lowercase();
        for word in ["should", "consider", "unhealthy", "reduce", "too much"] {
            assert!(!out.contains(word), "no advice: {word}");
        }
    }

    /// V9: stable keys, units named, and `null` -- never 0 -- when no fit
    /// was possible.
    #[test]
    fn json_names_its_units() {
        let out = json(&exact_session(32_000, 1_500, 40));
        assert!(out.contains("\"scale_milli\":1500"));
        assert!(out.contains("\"scale_unit\":\"thousandths_per_bytes4_unit\""));
        assert!(out.contains("\"band_unit\":\"tenths_of_percent\""));
        assert!(!out.contains('~'), "no tilde in json (V9)");
    }

    /// V47: `null` -- never 0 -- when no fit was possible, and `n` is
    /// reported either way so a reader can see WHY.
    #[test]
    fn json_nulls_an_absent_fit_rather_than_zeroing_it() {
        let thin = json(&exact_session(1_000, 1_000, 3));
        assert!(thin.contains("\"scale_milli\":null"));
        assert!(!thin.contains("\"scale_milli\":0"), "never a zero");
        assert!(thin.contains("\"turns\":3"), "n is always reported");
    }

    /// Rendering is readable and unambiguous: 1571 means 1.571x, 74 means
    /// 7.4%.
    #[test]
    fn the_units_render_as_written() {
        assert_eq!(scale_str(1_571), "1.571x");
        assert_eq!(scale_str(1_000), "1.000x");
        assert_eq!(band_str(74), "+-7.4%");
        assert_eq!(band_str(0), "+-0.0%");
    }

    /// V104: a NAMED miss is a usage error, an INFERRED one is silence.
    ///
    /// This asserted exit 0 for a named path that does not exist, citing
    /// V5 -- the defect B14 records, encoded as law. Report-only means
    /// never gating on CONTENT, never that a bad argument goes unreported.
    /// No usage anywhere is still absent rather than zero (V47); that is a
    /// different question and still exits 0.
    #[test]
    fn a_named_miss_is_a_usage_error() {
        let out = calibrate(&["/nonexistent/s.jsonl".to_owned()]);
        assert_eq!(out.code, 2, "a name that resolves to nothing");
        assert!(out.out.is_empty(), "nothing on stdout");
        assert!(out.err.contains("no session"), "{}", out.err);
    }

    /// V45: a real tier is refused -- no content is stored to tokenize.
    #[test]
    fn a_real_tier_is_refused() {
        let out = calibrate(&[fixture("minimal.jsonl"), "--bpe".to_owned()]);
        assert_eq!(out.code, 2);
        assert!(out.err.contains("no content is stored"));
        assert!(out.err.contains("calibrate"), "the message names the verb");
    }

    #[test]
    fn unknown_flags_and_bad_formats_are_usage_errors() {
        let f = fixture("minimal.jsonl");
        assert_eq!(calibrate(&[f.clone(), "--nope".to_owned()]).code, 2);
        assert_eq!(
            calibrate(&[f, "--format".to_owned(), "yaml".to_owned()]).code,
            2
        );
    }

    /// `-h` abbreviates, like every other verb. Tested on the formatter
    /// directly: no fixture is long enough to fit, so a test that went
    /// through `report` would assert nothing.
    #[test]
    fn human_sizes_abbreviate() {
        assert_eq!(size(200_000, true), "200k");
        assert_eq!(size(200_000, false), "200000");
    }

    /// The json path through the VERB, not just the renderer -- the
    /// dispatch and the format flag are part of the contract (V9).
    #[test]
    fn the_verb_renders_json_end_to_end() {
        let out = calibrate(&[
            fixture("minimal.jsonl"),
            "--format".to_owned(),
            "json".to_owned(),
        ]);
        assert_eq!(out.code, 0);
        assert!(out.out.contains("\"turns\":"));
        assert!(out.out.contains("\"scale_milli\":null"), "one turn: no fit");
    }

    /// `-h` and `-C` are accepted, matching the sibling runtime verbs.
    #[test]
    fn the_human_and_chdir_flags_are_accepted() {
        let out = calibrate(&[
            fixture("minimal.jsonl"),
            "-h".to_owned(),
            "-C".to_owned(),
            ".".to_owned(),
        ]);
        assert_eq!(out.code, 0);
    }

    /// A real fixture parses and reports without a fit: it has one turn,
    /// which is the honest answer rather than a fabricated factor.
    #[test]
    fn a_short_real_session_reports_n_only() {
        let out = calibrate(&[fixture("minimal.jsonl")]);
        assert_eq!(out.code, 0);
        assert!(out.out.contains("too few to fit"));
    }
}
