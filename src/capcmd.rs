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
    /// Rungs the caller ASKED for. Stored as a set, applied in ladder
    /// order (V50): the flag order is the caller's convenience and the
    /// ladder order is the invariant.
    asked: Vec<crate::ladder::Rung>,
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
    let (reduced, ran) = climb(req, input);
    let c = cut(&reduced, input, req.budget);
    let mut out = c.body.clone();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    match req.footer {
        Kind::Human => out.push_str(&human_footer(&c, &ran)),
        Kind::Json => out.push_str(&json_footer(&c, req.budget, &ran)),
    }
    out.push('\n');
    out
}

/// Climb the ladder until the budget is met, and report which rungs ran.
///
/// V50's two rules, and both are load-bearing. IN ORDER: the ladder is
/// sorted lossless -> lossiest, so a caller writing `--outline --strip`
/// still gets strip first; letting the flag order win would put structural
/// loss ahead of whitespace removal, which is the failure the ladder
/// exists to prevent. STOP AT THE BUDGET: the cheapest sufficient rung
/// wins, so a text that fits after `strip` is never deduped, and the
/// footer says only `strip` ran.
///
/// No budget means no rung runs at all. `cap` with no N is a
/// pass-through that reports (V49), and reducing text nobody asked to fit
/// would be a filter rewriting bytes for its own reasons.
fn climb(req: &Req, input: &str) -> (String, Vec<&'static str>) {
    let mut text = input.to_owned();
    let mut ran = Vec::new();
    let mut asked = req.asked.clone();
    asked.sort_unstable();
    asked.dedup();
    for rung in asked {
        if !over(len(&text), req.budget) {
            break;
        }
        text = rung.apply(&text);
        ran.push(rung.label());
    }
    (text, ran)
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

/// The prefix comes from the text AFTER the ladder ran; the totals come
/// from what the CALLER handed in.
///
/// Both, because "elided" has to mean "what your input had and this output
/// does not" -- measuring the elision against the reduced text would
/// report a file as whole while three rungs had rewritten it (V3).
fn cut(reduced: &str, original: &str, budget: Option<u64>) -> Cut {
    let body = kept_prefix(reduced, budget);
    Cut {
        kept_lines: lines(&body),
        total_bytes: len(original),
        total_lines: lines(original),
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
fn human_footer(c: &Cut, ran: &[&'static str]) -> String {
    let (t, m) = (mark(), DUMMY.label());
    if c.elided() || !ran.is_empty() {
        return format!(
            "[itok cap: kept {t}{} of {t}{} itok ({m}); {}]",
            c.kept_tokens(),
            c.input_tokens(),
            elision(c, ran),
        );
    }
    format!(
        "[itok cap: kept {t}{} itok ({m}); nothing elided]",
        c.kept_tokens()
    )
}

/// What was dropped and where to pick it up again (V50's named rung,
/// V51's selector).
fn elision(c: &Cut, ran: &[&'static str]) -> String {
    let mut names: Vec<&str> = ran.to_vec();
    if c.elided() {
        names.push("cap");
    }
    format!(
        "elided {} of {} lines, {} bytes; rungs: {}; {}",
        c.elided_lines(),
        c.total_lines,
        c.elided_bytes(),
        names.join(", "),
        resume_note(c, ran),
    )
}

/// Where to pick the stream up again, or why there is nowhere (V51).
///
/// A reduced text has no offset into the original, and saying so beats a
/// number that reads as usable.
fn resume_note(c: &Cut, ran: &[&'static str]) -> String {
    if !ran.is_empty() {
        return "no resume point: the text was reduced, so offsets do not \
                index the input"
            .to_owned();
    }
    format!(
        "resume: line {} (byte offset {})",
        c.resume_line(),
        c.kept_bytes()
    )
}

/// The json footer: the stable machine contract (V9). Numbers stay
/// numeric -- the tilde is a human rendering and never appears here, the
/// `estimated` flag carries that intent instead (V3).
fn json_footer(c: &Cut, budget: Option<u64>, ran: &[&'static str]) -> String {
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
        rungs(c, ran),
        resume(c, ran),
    )
}

fn span(tokens: u64, lines: u64, bytes: u64) -> String {
    format!("{{\"tokens\":{tokens},\"lines\":{lines},\"bytes\":{bytes}}}")
}

/// The rungs APPLIED (V50) -- empty when nothing was cut, since no rung
/// ran. A list, so T40's ladder appends rather than reshapes the field.
fn rungs(c: &Cut, ran: &[&'static str]) -> String {
    let mut all: Vec<String> = ran.iter().map(|r| format!("\"{r}\"")).collect();
    if c.elided() {
        all.push("\"cap\"".to_owned());
    }
    all.join(",")
}

/// The resume selector (V51), ABSENT when nothing was elided: no key
/// rather than a null one, the shape json.rs already uses for the absent
/// endpoint -- there is no resume point, not an unknown one.
fn resume(c: &Cut, ran: &[&'static str]) -> String {
    // A rung above `cap` REWRITES the text, so a line and byte offset
    // into it no longer index the caller's own stream. V51's promise is
    // about truncation; publishing a selector that points into text the
    // caller never had would be worse than publishing none (V47/V3).
    if !c.elided() || !ran.is_empty() {
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
        p if crate::ladder::Rung::parse(p).is_some() => {
            r.asked.extend(crate::ladder::Rung::parse(p));
        }
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
    /// Text that is dense in escapes and repeats, so each rung has
    /// something to do and the order is observable.
    fn noisy() -> String {
        let mut t = String::new();
        for _ in 0..6 {
            t.push_str("\u{1b}[32mrepeated line with padding\u{1b}[0m   \n");
        }
        for n in 0..6 {
            t.push_str(&format!("fn f{n}() {{\n    let body = {n};\n}}\n"));
        }
        t
    }

    /// V50: the LADDER order wins, not the flag order. `--outline
    /// --strip` still runs strip first -- letting the caller reorder it
    /// would put structural loss ahead of whitespace removal, which is
    /// the failure the ladder exists to prevent.
    #[test]
    fn the_ladder_order_wins_over_the_flag_order() {
        let out = cap(&args(&["4", "--outline", "--strip"]), &noisy()).out;
        let strip = out.find("strip").unwrap_or(usize::MAX);
        let outline = out.find("outline").unwrap_or(0);
        assert!(strip < outline, "{out}");
    }

    /// V50: the CHEAPEST SUFFICIENT rung wins. Once the text fits, the
    /// ladder stops, and the footer names only what ran.
    ///
    /// The budget is DERIVED from what strip leaves, rather than guessed:
    /// a hand-picked number that happened to sit below it would have the
    /// test asserting the opposite of its own name, which is the shape
    /// B24 records.
    #[test]
    fn the_ladder_stops_at_the_first_rung_that_fits() {
        let text = noisy();
        let stripped = crate::ladder::Rung::Strip.apply(&text);
        let budget = crate::estimate::dummy(len(&stripped)).to_string();
        let asked = args(&[&budget, "--strip", "--dedup", "--outline"]);
        let out = cap(&asked, &text).out;
        assert!(out.contains("rungs: strip"), "{out}");
        assert!(!out.contains("dedup"), "stopped after strip: {out}");
        assert!(!out.contains("outline"), "{out}");
    }

    /// V49: no budget is a pass-through that reports. Reducing text
    /// nobody asked to fit would be a filter rewriting bytes for its own
    /// reasons.
    #[test]
    fn no_budget_runs_no_rung_however_many_were_asked_for() {
        let text = noisy();
        let out = cap(&args(&["--strip", "--dedup", "--outline"]), &text).out;
        assert!(out.starts_with(&text), "body untouched: {out}");
        assert!(out.contains("nothing elided"), "{out}");
    }

    /// V51: a rung above `cap` REWRITES the text, so an offset into it
    /// does not index the caller's stream. Saying so beats a number that
    /// reads as usable.
    #[test]
    fn a_reduced_text_publishes_no_resume_point() {
        let reduced = cap(&args(&["4", "--outline"]), &noisy()).out;
        assert!(reduced.contains("no resume point"), "{reduced}");
        let truncated = cap(&args(&["4"]), &noisy()).out;
        assert!(truncated.contains("resume: line"), "{truncated}");
    }

    /// The elision is measured against what the CALLER handed in, not
    /// against the text the ladder left behind -- otherwise a file three
    /// rungs had rewritten would report as whole (V3).
    #[test]
    fn the_totals_are_the_callers_own_input() {
        let text = noisy();
        let out =
            cap(&args(&["4", "--outline", "--footer", "json"]), &text).out;
        let bytes = format!("\"bytes\":{}", text.len());
        assert!(out.contains(&bytes), "input span is the original: {out}");
    }

    /// V51: same input, same flags, same output. Every rung is a pure
    /// function, and the ladder that drives them is too.
    #[test]
    fn the_same_cut_twice_is_the_same_cut() {
        let once = cap(&args(&["4", "--strip", "--dedup"]), &noisy()).out;
        let twice = cap(&args(&["4", "--strip", "--dedup"]), &noisy()).out;
        assert_eq!(once, twice);
    }

    /// V50's rung names reach the machine contract too (V9).
    #[test]
    fn json_lists_every_rung_that_ran() {
        let out = cap(
            &args(&["4", "--strip", "--outline", "--footer", "json"]),
            &noisy(),
        )
        .out;
        assert!(
            out.contains("\"rungs\":[\"strip\",\"outline\",\"cap\"]"),
            "{out}"
        );
    }

    fn input() -> String {
        (0..10).map(|n| format!("line {n}x\n")).collect()
    }

    /// V49: the cut is by TOKENS. 8 bytes = 2 itok per line, so a budget
    /// of 6 keeps exactly 3 lines -- a byte- or line-shaped `head` would
    /// have taken a different amount.
    #[test]
    fn the_cut_is_by_tokens() {
        let c = cut(&input(), &input(), Some(6));
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
        let c = cut(&input(), &input(), Some(7));
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
        let c = cut(&input(), &input(), Some(6));
        assert_eq!(c.resume_line(), 4);
        assert_eq!(c.kept_bytes(), 24);
        let f = human_footer(&c, &[]);
        assert!(f.contains("resume: line 4 (byte offset 24)"), "{f}");
    }

    /// The selector is the ARITHMETIC complement of what was kept: total
    /// minus kept, in lines and bytes both.
    #[test]
    fn the_elided_counts_complement_the_kept_ones() {
        let c = cut(&input(), &input(), Some(6));
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
        assert!(
            human_footer(&cut(&input(), &input(), Some(6)), &[])
                .contains("rungs: cap")
        );
        let whole = json_footer(&cut(&input(), &input(), None), None, &[]);
        assert!(whole.contains("\"rungs\":[]"), "no rung ran: {whole}");
    }

    /// V3: the human number names its unit and its method, and carries
    /// the tilde of the crude tier.
    #[test]
    fn the_human_footer_names_unit_method_and_estimate() {
        let f = human_footer(&cut(&input(), &input(), Some(6)), &[]);
        assert!(f.contains("kept ~6 of ~20 itok"), "{f}");
        assert!(f.contains("(bytes/4)"), "{f}");
    }

    /// V9/V3: json carries the same intent structurally -- never a tilde
    /// in a numeric field.
    #[test]
    fn the_json_footer_is_the_machine_contract() {
        let j = json_footer(&cut(&input(), &input(), Some(6)), Some(6), &[]);
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
        let j = json_footer(&cut(&input(), &input(), None), None, &[]);
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
        let c = cut(&text, &text, Some(1));
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
        let c = cut("abcdef", "abcdef", None);
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
