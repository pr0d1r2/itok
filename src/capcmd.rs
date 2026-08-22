//! The `cap` command (V49): a stdin->stdout token filter that ANNOUNCES
//! what it cut. `head`/`tail` truncate SILENTLY, by lines or bytes; `cap`
//! truncates by TOKENS and appends a machine-parsable footer, which is why
//! it takes a different name rather than becoming an almost-`head` (V2).
//!
//! Usable with no agent and no hook -- `cmd | itok cap 10k` is the whole
//! product of this rung (V42/V49).
//!
//! The cut is at LINE boundaries: the longest whole-line prefix whose
//! `bytes/4` cost fits the budget. A partial line has no line-range resume
//! selector (V51) and no byte cut that stays the same across tiers, and a
//! single huge line is what the `elide`/`outline` rungs are for (V50, T40).
//! Today the only rung applied is `cap` itself, and the footer names it --
//! the field is a LIST so the ladder appends to it rather than reshaping it.
//!
//! Report-only, exit 0 always: the gate set is closed and `cap` is not in
//! it (V53). Pure -- no writes, no ambient state (V89), so re-running it on
//! the same input yields the same cut (V51/V5).
//!
//! The selector's two halves are not equally strong on non-UTF-8 input,
//! and the difference is stated rather than glossed (V3). Invalid bytes are
//! replaced on the way in, and a replacement character is 3 bytes where the
//! original was 1, so the BYTE offset indexes the decoded text. The LINE
//! number is exact either way -- replacement never adds or removes a
//! newline -- so `tail -n +<line>` resumes any stream, while `tail -c` is
//! for text.

use crate::cli::Output;
use crate::render::DUMMY;
use crate::units;

/// A parsed cap request. No budget means no truncation: the input passes
/// through and the footer still reports what it cost.
#[derive(Default, Debug, PartialEq, Eq)]
struct Req {
    budget: Option<u64>,
    footer: Kind,
}

/// Which footer to emit. `--footer`, not `--format`: the body is the
/// caller's own bytes passing through, so only the ANNOUNCEMENT has a
/// shape to choose (V9's json contract applies to the footer alone).
#[derive(Default, Debug, PartialEq, Eq)]
enum Kind {
    #[default]
    Human,
    Json,
}

pub(crate) fn cap(rest: &[String], input: &str) -> Output {
    match parse(rest) {
        Ok(req) => Output::ok(run(&req, input)),
        Err(e) => Output::usage_err(format!("itok: {e}")),
    }
}

/// The kept prefix, then the footer. The footer starts on its own line, so
/// a body without a trailing newline gets one -- an OUTPUT newline only,
/// never counted into the resume offset, which indexes the original input.
fn run(req: &Req, input: &str) -> String {
    let c = cut(input, req.budget);
    let mut out = c.body.clone();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    match req.footer {
        Kind::Human => out.push_str(&human_footer(&c)),
        Kind::Json => out.push_str(&json_footer(&c, req.budget)),
    }
    out.push('\n');
    out
}

// ------------------------------------------------------------- the cut

/// The kept prefix and the arithmetic the footer reports.
struct Cut {
    body: String,
    kept_lines: u64,
    total_bytes: u64,
    total_lines: u64,
}

impl Cut {
    fn kept_bytes(&self) -> u64 {
        len(&self.body)
    }

    fn kept_tokens(&self) -> u64 {
        crate::estimate::dummy(self.kept_bytes())
    }

    fn input_tokens(&self) -> u64 {
        crate::estimate::dummy(self.total_bytes)
    }

    /// Derived by SUBTRACTION so kept + elided == input exactly; taking
    /// `dummy()` of the elided bytes would disagree by a token, and a
    /// machine contract that does not add up is worse than a cruder one.
    fn elided_tokens(&self) -> u64 {
        self.input_tokens().saturating_sub(self.kept_tokens())
    }

    fn elided_bytes(&self) -> u64 {
        self.total_bytes.saturating_sub(self.kept_bytes())
    }

    fn elided_lines(&self) -> u64 {
        self.total_lines.saturating_sub(self.kept_lines)
    }

    fn elided(&self) -> bool {
        self.elided_lines() > 0
    }

    /// The resume selector's line: 1-based, the FIRST line not emitted, so
    /// `sed -n '<line>,$p'` continues where this run stopped (V51).
    fn resume_line(&self) -> u64 {
        self.kept_lines.saturating_add(1)
    }
}

fn cut(input: &str, budget: Option<u64>) -> Cut {
    let body = kept_prefix(input, budget);
    Cut {
        kept_lines: lines(&body),
        total_bytes: len(input),
        total_lines: lines(input),
        body,
    }
}

/// The longest whole-line prefix whose `bytes/4` cost fits the budget.
///
/// A PREFIX, not a greedy pick like `fit`'s (V20): a later line that would
/// still fit is deliberately dropped, because the resume selector promises
/// the remainder is contiguous from one offset.
fn kept_prefix(input: &str, budget: Option<u64>) -> String {
    let mut out = String::new();
    for l in input.split_inclusive('\n') {
        if over(len(&out).saturating_add(len(l)), budget) {
            break;
        }
        out.push_str(l);
    }
    out
}

/// Whether a prefix of `n` bytes costs more than the budget. No budget
/// means nothing is ever over -- `cap` with no N is a pass-through that
/// still reports (V49: it announces, it does not gate).
fn over(n: u64, budget: Option<u64>) -> bool {
    budget.is_some_and(|b| crate::estimate::dummy(n) > b)
}

/// Lines, counting a final unterminated line as one; empty input is 0.
fn lines(s: &str) -> u64 {
    count(s.split_inclusive('\n').count())
}

fn len(s: &str) -> u64 {
    count(s.len())
}

/// Saturating, never a panic: a token filter must not be the thing that
/// crashes (the crate denies arithmetic side effects for the same reason).
fn count(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

// ---------------------------------------------------------- the footer

/// The estimate marker, read from the tier rather than hardcoded so a
/// change of tier here cannot silently drop the `~` (V3).
fn mark() -> &'static str {
    if DUMMY.approximate { "~" } else { "" }
}

/// The human footer -- one bracketed line, ALWAYS emitted.
///
/// Always, even when nothing was elided: an absent footer is
/// indistinguishable from itok never having been in the pipe, so a reader
/// could not tell a whole document from a prefix. That is V44's
/// accounted-vs-total honesty, applied to a filter.
fn human_footer(c: &Cut) -> String {
    let (t, m) = (mark(), DUMMY.label());
    if c.elided() {
        return format!(
            "[itok cap: kept {t}{} of {t}{} itok ({m}); {}]",
            c.kept_tokens(),
            c.input_tokens(),
            elision(c),
        );
    }
    format!(
        "[itok cap: kept {t}{} itok ({m}); nothing elided]",
        c.kept_tokens()
    )
}

/// What was dropped and where to pick it up again (V50's named rung,
/// V51's selector).
fn elision(c: &Cut) -> String {
    format!(
        "elided {} of {} lines, {} bytes; rungs: cap; resume: line {} \
         (byte offset {})",
        c.elided_lines(),
        c.total_lines,
        c.elided_bytes(),
        c.resume_line(),
        c.kept_bytes(),
    )
}

/// The json footer: the stable machine contract (V9). Numbers stay
/// numeric -- the tilde is a human rendering and never appears here, the
/// `estimated` flag carries that intent instead (V3).
fn json_footer(c: &Cut, budget: Option<u64>) -> String {
    format!(
        "{{\"tool\":\"cap\"{},\"unit\":\"{}\",\"method\":\"{}\",\
         \"estimated\":{},\"kept\":{},\"input\":{},\"elided\":{},\
         \"rungs\":[{}]{}}}",
        budget_field(budget),
        crate::json::UNIT,
        DUMMY.tier,
        DUMMY.approximate,
        span(c.kept_tokens(), c.kept_lines, c.kept_bytes()),
        span(c.input_tokens(), c.total_lines, c.total_bytes),
        span(c.elided_tokens(), c.elided_lines(), c.elided_bytes()),
        rungs(c),
        resume(c),
    )
}

fn span(tokens: u64, lines: u64, bytes: u64) -> String {
    format!("{{\"tokens\":{tokens},\"lines\":{lines},\"bytes\":{bytes}}}")
}

/// The rungs APPLIED (V50) -- empty when nothing was cut, since no rung
/// ran. A list, so T40's ladder appends rather than reshapes the field.
fn rungs(c: &Cut) -> &'static str {
    if c.elided() { "\"cap\"" } else { "" }
}

/// The resume selector (V51), ABSENT when nothing was elided: no key
/// rather than a null one, the shape json.rs already uses for the absent
/// endpoint -- there is no resume point, not an unknown one.
fn resume(c: &Cut) -> String {
    if !c.elided() {
        return String::new();
    }
    format!(
        ",\"resume\":{{\"line\":{},\"byte\":{}}}",
        c.resume_line(),
        c.kept_bytes()
    )
}

fn budget_field(budget: Option<u64>) -> String {
    budget.map_or(String::new(), |n| format!(",\"budget\":{n}"))
}

// ----------------------------------------------------------- the parse

fn parse(rest: &[String]) -> Result<Req, String> {
    let mut r = Req::default();
    let mut i = 0usize;
    while let Some(a) = rest.get(i) {
        apply(&mut r, a, rest, &mut i)?;
        i = i.saturating_add(1);
    }
    Ok(r)
}

/// Apply one token: the footer flag, the one positional budget, or an
/// error. A second positional is REFUSED rather than ignored -- `cap`
/// reads stdin, so a path there is a misunderstanding worth naming, and
/// silently dropping an argument is B10's failure shape.
fn apply(
    r: &mut Req,
    a: &str,
    rest: &[String],
    i: &mut usize,
) -> Result<(), String> {
    match a {
        "--footer" => r.footer = kind(&val(rest, i)?)?,
        p if p.starts_with('-') => return Err(format!("unknown flag '{p}'")),
        n if r.budget.is_none() => r.budget = Some(units::parse(n)?),
        extra => {
            return Err(format!(
                "unexpected argument '{extra}' -- cap filters stdin, so it \
                 takes one budget and no paths"
            ));
        }
    }
    Ok(())
}

fn val(rest: &[String], i: &mut usize) -> Result<String, String> {
    *i = i.saturating_add(1);
    rest.get(*i)
        .cloned()
        .ok_or_else(|| "flag needs a value".to_owned())
}

fn kind(s: &str) -> Result<Kind, String> {
    match s {
        "human" => Ok(Kind::Human),
        "json" => Ok(Kind::Json),
        other => {
            Err(format!("unknown footer '{other}' (want 'human' or 'json')"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| (*s).to_owned()).collect()
    }

    /// Ten lines of exactly 8 bytes ("line Nx\n"): 80 bytes = 20 itok, so
    /// every line costs 2 and the boundaries land on round numbers.
    fn input() -> String {
        (0..10).map(|n| format!("line {n}x\n")).collect()
    }

    /// V49: the cut is by TOKENS. 8 bytes = 2 itok per line, so a budget
    /// of 6 keeps exactly 3 lines -- a byte- or line-shaped `head` would
    /// have taken a different amount.
    #[test]
    fn the_cut_is_by_tokens() {
        let c = cut(&input(), Some(6));
        assert_eq!(c.kept_lines, 3);
        assert_eq!(c.kept_tokens(), 6);
        assert_eq!(c.body, "line 0x\nline 1x\nline 2x\n");
    }

    /// A whole-line prefix: a budget BETWEEN line boundaries stops at the
    /// last line that fits and leaves the remainder unspent, rather than
    /// taking a partial line to fill it (V51 -- a partial line has no line
    /// selector to resume from).
    #[test]
    fn a_partial_line_is_never_emitted() {
        let c = cut(&input(), Some(7));
        assert_eq!(c.kept_lines, 3, "a 4th line would cost 8, over 7");
        assert_eq!(c.kept_tokens(), 6, "the odd token is left unspent");
        assert!(c.body.ends_with('\n'));
    }

    /// V49: the footer is always there, even with nothing elided -- its
    /// absence would be indistinguishable from itok not being in the pipe.
    #[test]
    fn a_cut_that_elides_nothing_still_announces() {
        let out = run(&Req::default(), &input());
        assert!(out.contains("[itok cap:"), "{out}");
        assert!(out.contains("nothing elided"), "{out}");
        assert!(out.starts_with("line 0x\n"), "body passes through");
    }

    /// V51: the selector names the first line NOT emitted and the byte
    /// offset into the ORIGINAL input, so the next read continues.
    #[test]
    fn the_footer_carries_the_resume_selector() {
        let c = cut(&input(), Some(6));
        assert_eq!(c.resume_line(), 4);
        assert_eq!(c.kept_bytes(), 24);
        let f = human_footer(&c);
        assert!(f.contains("resume: line 4 (byte offset 24)"), "{f}");
    }

    /// The selector is the ARITHMETIC complement of what was kept: total
    /// minus kept, in lines and bytes both.
    #[test]
    fn the_elided_counts_complement_the_kept_ones() {
        let c = cut(&input(), Some(6));
        assert_eq!(c.elided_lines(), 7);
        assert_eq!(c.elided_bytes(), 56);
        assert_eq!(c.kept_tokens().saturating_add(c.elided_tokens()), 20);
    }

    /// V51/V5: same input, same cut. Pure function, no ambient state.
    #[test]
    fn the_cut_is_idempotent() {
        let once = run(&Req::default(), &input());
        assert_eq!(once, run(&Req::default(), &input()));
        let capped = Req {
            budget: Some(6),
            ..Default::default()
        };
        assert_eq!(run(&capped, &input()), run(&capped, &input()));
    }

    /// V50: the footer names the rung that ran -- and names none when the
    /// input passed through untouched.
    #[test]
    fn the_applied_rung_is_named() {
        assert!(human_footer(&cut(&input(), Some(6))).contains("rungs: cap"));
        let whole = json_footer(&cut(&input(), None), None);
        assert!(whole.contains("\"rungs\":[]"), "no rung ran: {whole}");
    }

    /// V3: the human number names its unit and its method, and carries
    /// the tilde of the crude tier.
    #[test]
    fn the_human_footer_names_unit_method_and_estimate() {
        let f = human_footer(&cut(&input(), Some(6)));
        assert!(f.contains("kept ~6 of ~20 itok"), "{f}");
        assert!(f.contains("(bytes/4)"), "{f}");
    }

    /// V9/V3: json carries the same intent structurally -- never a tilde
    /// in a numeric field.
    #[test]
    fn the_json_footer_is_the_machine_contract() {
        let j = json_footer(&cut(&input(), Some(6)), Some(6));
        for key in [
            "\"tool\":\"cap\"",
            "\"budget\":6",
            "\"unit\":\"input_tokens\"",
            "\"method\":\"bytes/4\"",
            "\"estimated\":true",
            "\"resume\":{\"line\":4,\"byte\":24}",
        ] {
            assert!(j.contains(key), "missing {key}: {j}");
        }
        assert!(!j.contains('~'), "no tilde in json: {j}");
    }

    /// The three spans add up, and the absent ones stay absent (json.rs's
    /// no-key-rather-than-null rule).
    #[test]
    fn json_omits_what_is_absent() {
        let j = json_footer(&cut(&input(), None), None);
        assert!(!j.contains("\"resume\""), "nothing to resume from: {j}");
        assert!(!j.contains("\"budget\""), "no budget was given: {j}");
        assert!(
            j.contains("\"elided\":{\"tokens\":0,\"lines\":0,\"bytes\":0}")
        );
    }

    /// A budget too small for even the first line yields an empty body --
    /// honest, and the footer says so. Structure is `elide`'s job (T40).
    #[test]
    fn a_budget_below_the_first_line_keeps_nothing() {
        let out = run(
            &Req {
                budget: Some(1),
                ..Default::default()
            },
            &input(),
        );
        assert!(out.starts_with("[itok cap:"), "empty body: {out}");
        assert!(out.contains("resume: line 1 (byte offset 0)"), "{out}");
    }

    /// The LINE half of the selector survives a stream that was not valid
    /// UTF-8: replacement never adds or removes a newline, so the line
    /// numbers stay exact even though each replaced byte now measures 3.
    /// That asymmetry is why the module names `tail -n +<line>` as the
    /// selector that resumes ANY stream (V3: say which number is which).
    #[test]
    fn the_line_selector_survives_a_lossy_decode() {
        let raw = [0xffu8, b'\n', 0xfe, b'\n', 0xfd, b'\n'];
        let text = String::from_utf8_lossy(&raw).into_owned();
        let c = cut(&text, Some(1));
        assert_eq!(c.total_lines, 3, "three newlines, three lines");
        assert_eq!(c.kept_lines, 1, "4 bytes decoded is 1 itok");
        assert_eq!(c.resume_line(), 2);
        assert!(c.kept_bytes() > 2, "the decoded byte count is inflated");
    }

    #[test]
    fn empty_input_is_empty_output_plus_a_footer() {
        let out = run(&Req::default(), "");
        assert!(out.starts_with("[itok cap: kept ~0 itok"), "{out}");
    }

    /// A body without a trailing newline still gets the footer on its own
    /// line -- an OUTPUT newline only, which is why the offset below is
    /// the input's 6 bytes and not 7.
    #[test]
    fn an_unterminated_body_does_not_shift_the_offset() {
        let c = cut("abcdef", None);
        assert_eq!(c.kept_bytes(), 6);
        assert!(run(&Req::default(), "abcdef").starts_with("abcdef\n[itok"));
    }

    /// V18: the one unit grammar, and a bad count is a usage error.
    #[test]
    fn the_budget_uses_the_shared_unit_parser() {
        assert_eq!(parse(&args(&["10k"])), Ok(Req::default().budgeted(10_000)));
        assert!(parse(&args(&["xx"])).is_err());
    }

    #[test]
    fn the_footer_kind_is_a_flag_with_two_values() {
        let j = parse(&args(&["--footer", "json"])).ok();
        assert_eq!(j.map(|r| r.footer), Some(Kind::Json));
        assert!(parse(&args(&["--footer", "yaml"])).is_err());
        assert!(parse(&args(&["--footer"])).is_err(), "needs a value");
    }

    /// B10's rule: a stray argument is REFUSED with its reason, never
    /// silently dropped. `cap` reads stdin, so a path is a real mistake.
    #[test]
    fn a_second_positional_is_refused_with_its_reason() {
        let e = parse(&args(&["10k", "SPEC.md"])).err().unwrap_or_default();
        assert!(e.contains("SPEC.md"), "names it: {e}");
        assert!(e.contains("stdin"), "says why: {e}");
        assert!(parse(&args(&["--nope"])).is_err());
    }

    /// V53/V5: report-only. Even a total elision exits 0 -- `cap` is not
    /// in the closed gate set.
    #[test]
    fn cap_never_gates() {
        assert_eq!(cap(&args(&["0"]), &input()).code, 0);
        assert_eq!(cap(&args(&["--bogus"]), &input()).code, 2, "usage only");
    }

    impl Req {
        fn budgeted(self, n: u64) -> Self {
            Self {
                budget: Some(n),
                ..self
            }
        }
    }
}
