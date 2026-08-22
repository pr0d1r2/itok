//! The `rate` command: pre-formatted throughput string for a statusline
//! badge (V46). Report-only, exit 0 (V5). Aggregates, never verdicts
//! (V59). Per-metric ANSI color from `itok.toml` (V109).

use crate::args::Format;
use crate::cli::Output;
use crate::session::Session;
use crate::tracecmd::{Origin, value};

#[derive(Default)]
struct Raw {
    session: Option<String>,
    /// `None` = not asked for, so the DEFAULT still applies -- which is
    /// `auto` normally and `always` under `--statusline`. An explicit
    /// `--color` therefore wins regardless of flag order, instead of the
    /// two settings racing.
    color: Option<ColorMode>,
    format: Format,
    chdir: Option<String>,
    statusline: bool,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
enum ColorMode {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Default)]
struct RateConfig {
    turn: Option<Threshold>,
    total: Option<Threshold>,
    hour: Option<Threshold>,
    day: Option<Threshold>,
}

#[derive(Debug, Clone, Copy)]
struct Threshold {
    green: u64,
    amber: u64,
}

#[derive(serde::Deserialize, Default)]
struct TomlConfig {
    rate: Option<TomlRate>,
}

#[derive(serde::Deserialize, Default)]
struct TomlRate {
    turn: Option<TomlThreshold>,
    total: Option<TomlThreshold>,
    hour: Option<TomlThreshold>,
    day: Option<TomlThreshold>,
}

#[derive(serde::Deserialize)]
struct TomlThreshold {
    green: u64,
    amber: u64,
}

struct Throughput {
    last_turn: u64,
    total: u64,
    per_hour: Option<u64>,
    per_day: Option<u64>,
    turns: usize,
    age_seconds: u64,
    active_seconds: u64,
}

pub(crate) fn rate(rest: &[String], input: crate::cli::Input) -> Output {
    match parse(rest) {
        Ok(mut raw) => {
            if raw.statusline
                && let Err(o) = adopt_payload(&mut raw, input)
            {
                return o;
            }
            run(&raw)
        }
        Err(e) => Output::usage_err(format!("itok: {e}")),
    }
}

/// Take the session and directory from the harness's statusline payload.
///
/// Read HERE and not in `run`, so `input()` is called only when the flag
/// asked for it: a bare `itok rate` must never block waiting on a pipe
/// nobody is writing to.
///
/// An explicit positional session still wins -- the flag SUPPLIES a
/// default source, it does not seize one the caller named.
fn adopt_payload(
    raw: &mut Raw,
    input: crate::cli::Input,
) -> Result<(), Output> {
    let payload = crate::hook::statusline(&input());
    let Some(transcript) = payload.transcript else {
        return Err(Output::usage_err(
            "itok: --statusline: no `transcript_path` in the payload on stdin"
                .to_owned(),
        ));
    };
    raw.session.get_or_insert(transcript);
    if let Some(cwd) = payload.cwd {
        raw.chdir.get_or_insert(cwd);
    }
    Ok(())
}

fn run(raw: &Raw) -> Output {
    let (session, origin) = match session_of(raw) {
        Ok(v) => v,
        Err(o) => return o,
    };
    let Some(tp) = throughput(&session) else {
        return too_few(raw, &session);
    };
    format_output(raw, &tp, &origin)
}

/// A session too short to have a rate, answered in the SHAPE the caller
/// asked for.
///
/// The badge hides (SPEC section I: 0-1 turns = empty output) because a
/// statusline widget announcing its own silence is worse than no widget.
/// That rule is about the BADGE, and it was governing json too: `rate
/// <session> --format json` emitted zero bytes at exit 0, so a consumer
/// piping into `jq` got a parse error rather than an answer.
///
/// json therefore gets one object with the keys it always has and `null`
/// where nothing was measured -- V9's one-shape rule, and the same answer
/// `calibrate` already gives when its sample cannot support a fit ("keys
/// stay so a parser learns one shape, and `null` is json's own word for
/// not measured"). One verb answering this differently from its sibling is
/// B11's class, and this is the sibling it had not reached.
fn too_few(raw: &Raw, session: &Session) -> Output {
    match raw.format {
        Format::Json => Output::ok(json(&unmeasured(session))),
        Format::Human => Output::ok(String::new()),
    }
}

/// What a session too short to have a rate DID show: its turn count and
/// the tokens it billed. Everything the sample cannot support is `None`.
fn unmeasured(session: &Session) -> Throughput {
    Throughput {
        last_turn: session
            .turns
            .last()
            .and_then(crate::session::Turn::billed_input)
            .unwrap_or(0),
        total: session.billed_input(),
        per_hour: None,
        per_day: None,
        turns: session.turns.len(),
        age_seconds: 0,
        active_seconds: 0,
    }
}

fn format_output(raw: &Raw, tp: &Throughput, origin: &Origin) -> Output {
    let note = crate::tracecmd::origin_note(origin);
    let config = load_config(raw.chdir.as_deref());
    let want_color = want_color(color_of(raw));
    Output::ok(match raw.format {
        Format::Json => json(tp),
        Format::Human if raw.statusline => {
            badge(human(tp, &config, want_color))
        }
        Format::Human => human(tp, &config, want_color) + &note,
    })
}

/// The mode asked for, or the default for how this was invoked.
///
/// `--statusline` defaults to `always` because the harness CAPTURES the
/// string: `auto` probes for a tty, finds none, and would strip the color
/// from every badge -- a default that is right for a pipe is wrong for a
/// display surface being fed through one.
fn color_of(raw: &Raw) -> ColorMode {
    raw.color.unwrap_or(if raw.statusline {
        ColorMode::Always
    } else {
        ColorMode::Auto
    })
}

/// The badge, wrapped for a statusline. Empty stays EMPTY -- a session
/// with nothing to report hides the badge (V3), and `(itok:)` around
/// nothing is a widget announcing its own silence.
///
/// The origin note is dropped here: it explains WHICH session was picked,
/// and under `--statusline` the payload named it outright, so there is no
/// inference left to disclose.
fn badge(line: String) -> String {
    let body = line.trim_end_matches('\n');
    if body.is_empty() {
        return String::new();
    }
    format!("(itok:{body})")
}

fn want_color(mode: ColorMode) -> bool {
    match mode {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => is_tty(),
    }
}

fn session_of(raw: &Raw) -> Result<(Session, Origin), Output> {
    crate::tracecmd::session_at(raw.session.as_deref(), raw.chdir.as_deref())
}

fn throughput(session: &Session) -> Option<Throughput> {
    if session.turns.len() < 2 {
        return None;
    }
    let last = session.turns.last()?;
    let first = session.turns.first()?;
    Some(tp_from(Sample {
        last_turn: last.billed_input().unwrap_or(0),
        total: session.billed_input(),
        span: age_seconds(&first.ts, &last.ts),
        active: active_seconds(&session.turns),
        turns: session.turns.len(),
    }))
}

/// What the badge is computed FROM: the two token totals and the two
/// clocks. Grouped because `tp_from` would otherwise take five
/// arguments, and because span and active are easy to swap by accident
/// when they sit side by side as bare `u64`s.
struct Sample {
    last_turn: u64,
    total: u64,
    span: u64,
    active: u64,
    turns: usize,
}

fn tp_from(s: Sample) -> Throughput {
    Throughput {
        last_turn: s.last_turn,
        total: s.total,
        per_hour: rate_over(s.total, s.active, HOUR),
        per_day: rate_over(s.total, s.active, DAY),
        turns: s.turns,
        age_seconds: s.span,
        active_seconds: s.active,
    }
}

/// V111: the working clock -- every gap between consecutive turns, each
/// capped at `IDLE_CAP`, summed. The span between the first and last
/// stamp counts sleep; this counts work, and truncates the gap where a
/// human left rather than guessing whether they did.
///
/// N turns yield N-1 gaps, so the first turn is billed with no time
/// credited and the rate runs high by 1/N -- stated here because a short
/// session is exactly where that shows, and V110 already marks it.
fn active_seconds(turns: &[crate::session::Turn]) -> u64 {
    turns
        .iter()
        .zip(turns.iter().skip(1))
        .map(|(a, b)| gap(&a.ts, &b.ts))
        .fold(0u64, u64::saturating_add)
}

/// One inter-turn gap, credited up to the cap. An unreadable stamp
/// reads 0 here, exactly as it does for the span (V5).
fn gap(before: &str, after: &str) -> u64 {
    parse_ts(after)
        .saturating_sub(parse_ts(before))
        .min(IDLE_CAP)
}

/// The two periods the badge projects onto (V110), and the longest gap
/// credited as work rather than idle (V111 -- 300s is at or above the
/// 95th percentile of the inter-turn gap in every session measured).
const HOUR: u64 = 3_600;
const DAY: u64 = 86_400;
const IDLE_CAP: u64 = 300;

/// A rate, or NOTHING when no working time was measured.
///
/// The floor this replaces was `if active == 0 { 1 }`, and a fabricated
/// one-second denominator is not a conservative default: it multiplies the
/// session total by 3600 and by 86400, so two turns inside one wall-clock
/// second published 104 tokens as `375k/h`. B24 already recorded that exact
/// amplifier -- "the 1-second floor turned that into `132590m/h`" -- and
/// fixed only the stamp reader feeding it. B26 is the floor itself.
///
/// `None` rather than 0, because 0 is a MEASUREMENT (a session that billed
/// nothing) and this is the absence of one. V47/V92 is the rule already
/// written for the case: an absent number is a dash, never a zero. V110's
/// tilde cannot cover it either -- a mark on a fabricated value still asks
/// the reader to believe the digits.
fn rate_over(total: u64, active: u64, period: u64) -> Option<u64> {
    (active > 0).then(|| extrapolate(total, active, period))
}

fn extrapolate(total: u64, age: u64, period: u64) -> u64 {
    total
        .checked_mul(period)
        .and_then(|v| v.checked_div(age))
        .unwrap_or(0)
}

fn age_seconds(first: &str, last: &str) -> u64 {
    parse_ts(last).saturating_sub(parse_ts(first))
}

/// Epoch seconds from the RFC 3339 stamp a transcript carries
/// (`2026-08-15T06:55:39.102Z`). Fixed offsets over that fixed shape, not
/// a date parser and not a calendar crate -- the whole need is one
/// subtraction between two machine-written stamps. A stamp of any other
/// shape reads as 0, so `age_seconds` collapses to 0 and the badge still
/// prints: rate is report-only (V5), and an unreadable clock must not
/// become an error the caller has to handle. Pre-1970 stamps clamp their
/// date to the epoch day for the same reason -- no transcript carries
/// one, so the clamp costs nothing a caller can observe.
fn parse_ts(ts: &str) -> u64 {
    let days = days_from_civil(num(ts, 0..4), num(ts, 5..7), num(ts, 8..10));
    let secs = num(ts, 11..13)
        .saturating_mul(3600)
        .saturating_add(num(ts, 14..16).saturating_mul(60))
        .saturating_add(num(ts, 17..19));
    days.saturating_mul(86_400).saturating_add(secs)
}

/// One fixed-width numeric field, or 0 when it is absent or not a number.
fn num(ts: &str, at: std::ops::Range<usize>) -> u64 {
    ts.get(at).and_then(|s| s.parse().ok()).unwrap_or(0)
}

/// Days since 1970-01-01 for a civil date (Hinnant's `days_from_civil`).
/// Saturating throughout: `arithmetic_side_effects` is denied, and a
/// garbled date must yield a number, never a panic.
fn days_from_civil(y: u64, m: u64, d: u64) -> u64 {
    let shifted = if m <= 2 { y.saturating_sub(1) } else { y };
    let era = shifted.checked_div(400).unwrap_or(0);
    let yoe = shifted.saturating_sub(era.saturating_mul(400));
    era.saturating_mul(146_097)
        .saturating_add(day_of_era(yoe, day_of_year(m, d)))
        .saturating_sub(719_468)
}

/// Day of the year counted from March 1. That shift puts the leap day
/// last, which is what collapses the month lengths to one linear formula.
fn day_of_year(m: u64, d: u64) -> u64 {
    let mp = m.saturating_add(9).checked_rem(12).unwrap_or(0);
    mp.saturating_mul(153)
        .saturating_add(2)
        .checked_div(5)
        .unwrap_or(0)
        .saturating_add(d)
        .saturating_sub(1)
}

/// Day within the 400-year era: whole years, plus their leap days.
fn day_of_era(yoe: u64, doy: u64) -> u64 {
    let leaps = yoe
        .checked_div(4)
        .unwrap_or(0)
        .saturating_sub(yoe.checked_div(100).unwrap_or(0));
    yoe.saturating_mul(365)
        .saturating_add(leaps)
        .saturating_add(doy)
}

/// V108: ceiling, never floor.
pub(crate) fn shorten(n: u64) -> String {
    if n <= 99 {
        n.to_string()
    } else if n <= 999_000 {
        let k = n.saturating_add(999).checked_div(1000).unwrap_or(0);
        format!("{k}k")
    } else {
        let m = n
            .saturating_add(999_999)
            .checked_div(1_000_000)
            .unwrap_or(0);
        format!("{m}m")
    }
}

fn human(tp: &Throughput, config: &RateConfig, color: bool) -> String {
    let age = tp.active_seconds;
    let cells = [
        (Some(tp.last_turn), "", "", config.turn),
        (Some(tp.total), "/total", "", config.total),
        (tp.per_hour, "/h", mark(age, HOUR), config.hour),
        (tp.per_day, "/d", mark(age, DAY), config.day),
    ];
    let parts = cells.map(|cell| metric(cell, color));
    format!("{}\n", parts.join(","))
}

/// V110: the `~` on a value whose period the SAMPLE does not cover.
///
/// An hour-rate off twenty minutes is a projection, not a measurement,
/// and V3 has marked crude numbers this way since the first estimator.
/// Empty once the age covers the period -- the mark says the sample is
/// short, so it must disappear when the sample stops being short.
fn mark(age: u64, period: u64) -> &'static str {
    if age < period { "~" } else { "" }
}

/// One badge value: the number or its ABSENCE, its suffix, its projection
/// mark, and the thresholds that color it. Grouped because the limit on
/// arguments is four and the mark is the fifth thing every value carries.
type Cell = (Option<u64>, &'static str, &'static str, Option<Threshold>);

/// An absent value renders as `-`, uncolored and unmarked (V47/V92).
///
/// Uncolored because a threshold sorts a MEASUREMENT into a band and there
/// is nothing here to sort. Unmarked because V110's `~` says a sample is
/// too short for the period it names, which is a claim about a number that
/// exists.
fn metric(cell: Cell, color: bool) -> String {
    let (value, suffix, mark, threshold) = cell;
    let Some(value) = value else {
        return format!("-{suffix}");
    };
    let text = format!("{mark}{}{suffix}", shorten(value));
    colored(text, value, threshold, color)
}

fn colored(
    text: String,
    value: u64,
    threshold: Option<Threshold>,
    color: bool,
) -> String {
    if !color {
        return text;
    }
    let Some(th) = threshold else {
        return text;
    };
    let code = ansi_for(value, th);
    format!("{code}{text}\x1b[0m")
}

fn ansi_for(value: u64, th: Threshold) -> &'static str {
    if value < th.green {
        "\x1b[32m"
    } else if value < th.amber {
        "\x1b[33m"
    } else {
        "\x1b[31m"
    }
}

/// An unmeasurable rate is `null`, which is what the human badge's `-`
/// says in the shape json has for it (V9/V47). The alternative was a
/// number derived from a one-second denominator nobody measured, sitting
/// beside an `active_seconds` of 0 that contradicts it -- a reader could
/// not reproduce `per_hour` from the object publishing it (B26).
fn or_null(v: Option<u64>) -> String {
    v.map_or_else(|| "null".to_owned(), |n| n.to_string())
}

/// V110 lands here as a BOOLEAN per projected metric, not as a tilde in
/// a numeric field -- json carries the same intent structurally (V9).
///
/// The booleans describe the SAMPLE, not the value, so they stay honest
/// beside a `null`: zero working time covers neither period.
fn json(tp: &Throughput) -> String {
    let hour = tp.active_seconds < HOUR;
    let day = tp.active_seconds < DAY;
    format!(
        "{{\"last_turn\":{},\"total\":{},\"per_hour\":{},\
         \"per_day\":{},\"turns\":{},\"age_seconds\":{},\
         \"active_seconds\":{},\"projected_hour\":{hour},\
         \"projected_day\":{day}}}\n",
        tp.last_turn,
        tp.total,
        or_null(tp.per_hour),
        or_null(tp.per_day),
        tp.turns,
        tp.age_seconds,
        tp.active_seconds,
    )
}

fn load_config(chdir: Option<&str>) -> RateConfig {
    let Some(text) = read_config(chdir) else {
        return RateConfig::default();
    };
    parse_config(&text)
}

fn parse_config(text: &str) -> RateConfig {
    let Ok(config) = basic_toml::from_str::<TomlConfig>(text) else {
        return RateConfig::default();
    };
    let Some(rate) = config.rate else {
        return RateConfig::default();
    };
    config_from(rate)
}

fn config_from(rate: TomlRate) -> RateConfig {
    RateConfig {
        turn: rate.turn.map(to_threshold),
        total: rate.total.map(to_threshold),
        hour: rate.hour.map(to_threshold),
        day: rate.day.map(to_threshold),
    }
}

fn to_threshold(t: TomlThreshold) -> Threshold {
    Threshold {
        green: t.green,
        amber: t.amber,
    }
}

fn read_config(chdir: Option<&str>) -> Option<String> {
    let root = std::path::PathBuf::from(chdir.unwrap_or(".")).join("itok.toml");
    if let Ok(text) = std::fs::read_to_string(&root) {
        return Some(text);
    }
    read_global_config()
}

fn read_global_config() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let path = std::path::PathBuf::from(home).join(".config/itok/config.toml");
    std::fs::read_to_string(path).ok()
}

/// `auto` asks about the stream the BADGE lands on, which is stdout.
///
/// It probed stderr, and both directions were wrong: `itok rate > badge`
/// from a terminal wrote ANSI escapes into the file, because stderr was
/// still a tty; `itok rate 2>/dev/null` in a terminal stripped the color
/// from a tty stdout. `main.rs` prints `o.out` to stdout and `o.err` to
/// stderr, so the question "will anything render these escapes" is a
/// question about stdout and nothing else.
fn is_tty() -> bool {
    std::io::IsTerminal::is_terminal(&std::io::stdout())
}

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
        "--bpe" | "--ollama" => {
            return Err(crate::tracecmd::no_real_tier(a, "rate"));
        }
        "--color" => raw.color = Some(parse_color(&value(it, a)?)?),
        "--format" => raw.format = crate::tracecmd::format_of(&value(it, a)?)?,
        "-C" => raw.chdir = Some(value(it, a)?),
        "--statusline" => raw.statusline = true,
        _ => return apply_positional(a, raw),
    }
    Ok(())
}

fn parse_color(s: &str) -> Result<ColorMode, String> {
    match s {
        "auto" => Ok(ColorMode::Auto),
        "always" => Ok(ColorMode::Always),
        "never" => Ok(ColorMode::Never),
        other => Err(format!("unknown color mode '{other}'")),
    }
}

fn apply_positional(a: &str, raw: &mut Raw) -> Result<(), String> {
    if a.starts_with('-') {
        return Err(format!("unknown flag {a}"));
    }
    raw.session = Some(a.to_owned());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Turn;

    const ZERO_TP: Throughput = Throughput {
        last_turn: 0,
        total: 0,
        per_hour: Some(0),
        per_day: Some(0),
        turns: 0,
        age_seconds: 0,
        active_seconds: 0,
    };

    #[test]
    fn shorten_raw_digits_below_100() {
        assert_eq!(shorten(0), "0");
        assert_eq!(shorten(42), "42");
        assert_eq!(shorten(99), "99");
    }

    #[test]
    fn shorten_ceiling_k_small() {
        assert_eq!(shorten(100), "1k");
        assert_eq!(shorten(900), "1k");
        assert_eq!(shorten(1_000), "1k");
        assert_eq!(shorten(1_001), "2k");
        assert_eq!(shorten(1_500), "2k");
    }

    #[test]
    fn shorten_ceiling_k_large() {
        assert_eq!(shorten(47_000), "47k");
        assert_eq!(shorten(120_000), "120k");
        assert_eq!(shorten(900_000), "900k");
        assert_eq!(shorten(999_000), "999k");
    }

    #[test]
    fn shorten_ceiling_m_range() {
        assert_eq!(shorten(999_001), "1m");
        assert_eq!(shorten(1_000_000), "1m");
        assert_eq!(shorten(1_500_000), "2m");
        assert_eq!(shorten(22_000_000), "22m");
    }

    #[test]
    fn shorten_never_produces_1000k() {
        for n in [999_000, 999_001, 999_499, 999_500, 1_000_000] {
            let s = shorten(n);
            assert!(!s.contains("1000k"), "{n} -> {s}");
        }
    }

    fn session_with_turns(inputs: &[(u64, &str)]) -> Session {
        let mut s = Session::default();
        for (input, ts) in inputs {
            s.turns.push(Turn {
                input: Some(*input),
                ts: (*ts).to_owned(),
                ..Turn::default()
            });
        }
        s
    }

    /// The stamps below are RFC 3339 because that is what a transcript
    /// carries. B-record: the first cut of these fixtures used epoch
    /// digits, which no reader ever writes -- every rate test passed
    /// while `age_seconds` was 0 on every real session.
    const T0: &str = "2026-08-15T06:00:00.000Z";
    const T1H: &str = "2026-08-15T07:00:00.000Z";

    /// The badge hides on a short session; json still answers (V9).
    ///
    /// `--format json` used to emit ZERO BYTES here, so a consumer piping
    /// into `jq` got a parse error while the exit code said success. The
    /// keys are the ones every other `rate` object carries, `null` where
    /// nothing was measured -- the shape `calibrate` already returns when
    /// its own sample is too short.
    #[test]
    fn json_answers_a_short_session_with_an_object() {
        let one = session_with_turns(&[(1_111, "2026-08-22T10:00:00.000Z")]);
        let raw = Raw {
            format: Format::Json,
            ..Raw::default()
        };
        let out = too_few(&raw, &one).out;
        assert!(out.starts_with('{'), "one object, not silence: {out}");
        assert!(out.contains("\"turns\":1"), "{out}");
        assert!(out.contains("\"last_turn\":1111"), "{out}");
        assert!(out.contains("\"total\":1111"), "{out}");
        assert!(out.contains("\"per_hour\":null"), "{out}");
        assert!(out.contains("\"per_day\":null"), "{out}");
    }

    /// The human badge keeps its silence: a widget rendering `(itok:)`
    /// around nothing announces only itself (V3, section I).
    #[test]
    fn the_badge_stays_hidden_on_a_short_session() {
        let one = session_with_turns(&[(1_111, "2026-08-22T10:00:00.000Z")]);
        let out = too_few(&Raw::default(), &one).out;
        assert!(out.is_empty(), "badge hides: {out}");
    }

    #[test]
    fn throughput_needs_at_least_two_turns() {
        let s = session_with_turns(&[(1000, T0)]);
        assert!(throughput(&s).is_none());
        let empty = Session::default();
        assert!(throughput(&empty).is_none());
    }

    #[test]
    fn throughput_computes_totals() {
        let s = session_with_turns(&[(1_000, T0), (2_000, T1H)]);
        let tp = throughput(&s);
        assert!(tp.is_some());
        let tp = tp.unwrap_or(ZERO_TP);
        assert_eq!(tp.last_turn, 2_000);
        assert_eq!(tp.total, 3_000);
        assert_eq!(tp.turns, 2);
    }

    /// The span stays reported, but it is NOT what divides: one gap of
    /// an hour is credited `IDLE_CAP`, so 3000 tokens over 300 working
    /// seconds is 36k/h (V111).
    #[test]
    fn rates_divide_by_active_time_not_span() {
        let s = session_with_turns(&[(1_000, T0), (2_000, T1H)]);
        let tp = throughput(&s).unwrap_or(ZERO_TP);
        assert_eq!(tp.age_seconds, 3600, "span still reported");
        assert_eq!(tp.active_seconds, IDLE_CAP, "the gap is capped");
        assert_eq!(tp.per_hour, Some(36_000));
        assert_eq!(tp.per_day, Some(864_000));
    }

    /// Every gap is capped on its own, so a session is the SUM of its
    /// working gaps and one long absence cannot swamp the rest.
    #[test]
    fn each_gap_is_capped_on_its_own() {
        let turns = session_with_turns(&[
            (1, "2026-08-15T06:00:00.000Z"),
            (1, "2026-08-15T06:00:30.000Z"),
            (1, "2026-08-15T06:02:00.000Z"),
            (1, "2026-08-16T06:02:00.000Z"),
            (1, "2026-08-16T06:03:00.000Z"),
        ]);
        let active = active_seconds(&turns.turns);
        assert_eq!(active, 30 + 90 + IDLE_CAP + 60);
    }

    /// A day away counts as `IDLE_CAP`, not as a day: this is the
    /// 337.9h-span session in miniature, whose wall-clock rate
    /// understated the working rate ~20x.
    #[test]
    fn an_absence_is_truncated_not_counted() {
        let s = session_with_turns(&[
            (1_000, "2026-08-15T06:00:00.000Z"),
            (2_000, "2026-08-18T06:00:00.000Z"),
        ]);
        let tp = throughput(&s).unwrap_or(ZERO_TP);
        assert_eq!(tp.age_seconds, 3 * 86_400, "three days of span");
        assert_eq!(tp.active_seconds, IDLE_CAP, "five minutes of work");
    }

    /// Unreadable stamps leave no working time at all. The clock clamps
    /// so nothing divides by zero, and V110 marks every period the
    /// non-existent sample cannot cover.
    #[test]
    fn an_unreadable_clock_leaves_no_active_time() {
        let s = session_with_turns(&[(1_000, "nope"), (2_000, "also-nope")]);
        let tp = throughput(&s).unwrap_or(ZERO_TP);
        assert_eq!(tp.active_seconds, 0);
        assert_eq!(tp.per_hour, None, "no working time, no rate");
        assert_eq!(tp.per_day, None);
        let out = human(&tp, &RateConfig::default(), false);
        assert!(out.contains("-/h") && out.contains("-/d"), "{out}");
        assert!(!out.contains('~'), "nothing to project from: {out}");
    }

    /// B26: a fabricated denominator is not a conservative default.
    ///
    /// The floor here was `if active == 0 { 1 }`, which multiplied the
    /// session total by 3600 and by 86400 -- so 3000 tokens over an
    /// unmeasurable clock published as `10m/h`. This test asserts the
    /// MAGNITUDE, which is what the assertions above it never did: the old
    /// test checked `active_seconds == 0` and that a tilde was present, and
    /// both were true of the garbage.
    #[test]
    fn a_clockless_rate_never_publishes_an_amplified_number() {
        let s = session_with_turns(&[(1_000, "nope"), (2_000, "also-nope")]);
        let tp = throughput(&s).unwrap_or(ZERO_TP);
        let out = human(&tp, &RateConfig::default(), false);
        assert!(!out.contains('m'), "no megatoken rate anywhere: {out}");
        let js = json(&tp);
        assert!(js.contains("\"per_hour\":null"), "{js}");
        assert!(js.contains("\"per_day\":null"), "{js}");
        assert!(js.contains("\"active_seconds\":0"), "{js}");
    }

    /// The reachable case, with no broken clock in sight: `parse_ts`
    /// truncates to seconds, so two turns inside one wall-clock second --
    /// ordinary at session start -- leave zero active time. Under the old
    /// floor that was the amplifier, not an edge case.
    #[test]
    fn two_turns_in_one_second_publish_no_rate() {
        let s = session_with_turns(&[
            (52, "2026-08-22T10:00:00.100Z"),
            (52, "2026-08-22T10:00:00.900Z"),
        ]);
        let tp = throughput(&s).unwrap_or(ZERO_TP);
        assert_eq!(tp.active_seconds, 0, "same second, no gap credited");
        assert_eq!(tp.total, 104);
        assert_eq!(tp.per_hour, None, "104 tokens are not 375k/h");
        assert_eq!(tp.per_day, None);
    }

    /// The stamp shape pinned against a value computed elsewhere, so a
    /// reimplementation of `days_from_civil` cannot drift unnoticed.
    #[test]
    fn rfc3339_stamps_become_epoch_seconds() {
        assert_eq!(parse_ts("1970-01-01T00:00:00.000Z"), 0);
        assert_eq!(parse_ts("1970-01-02T00:00:01.000Z"), 86_401);
        assert_eq!(parse_ts("2000-03-01T00:00:00.000Z"), 951_868_800);
        assert_eq!(parse_ts("2026-08-15T06:55:39.102Z"), 1_786_776_939);
    }

    /// The leap day lands on the right side of the March-shift, in a
    /// leap year and in the century year that is NOT one.
    #[test]
    fn leap_days_are_counted() {
        let day = parse_ts("2024-03-01T00:00:00.000Z")
            .saturating_sub(parse_ts("2024-02-29T00:00:00.000Z"));
        assert_eq!(day, 86_400, "2024-02-29 exists");
        let century = parse_ts("2100-03-01T00:00:00.000Z")
            .saturating_sub(parse_ts("2100-02-28T00:00:00.000Z"));
        assert_eq!(century, 86_400, "2100 is not a leap year");
    }

    /// A stamp of any other shape degrades to 0 (V5): no panic, no error.
    /// The epoch-digit case is the exact fixture shape that hid this bug.
    #[test]
    fn an_unreadable_stamp_is_no_age() {
        assert_eq!(parse_ts(""), 0);
        assert_eq!(parse_ts("not-a-timestamp"), 0);
        assert_eq!(parse_ts("1000000000"), 0);
        let pre = parse_ts("1900-06-01T12:00:00.000Z");
        assert!(pre < 86_400, "pre-epoch clamps to the epoch day: {pre}");
    }

    /// The sample here is 3120 seconds: under an hour, so BOTH derived
    /// values are projections and both wear the mark (V110).
    #[test]
    fn human_output_shape() {
        let out = human(&sample_tp(), &RateConfig::default(), false);
        assert_eq!(out, "3k,46k/total,~13k/h,~280k/d\n");
    }

    /// The mark is about the DENOMINATOR reaching the period, so it
    /// flips exactly at the period and nowhere else. Both boundaries
    /// pinned from the side that still projects and the side that does
    /// not -- B24 is what a fixture cut to the code costs.
    #[test]
    fn mark_flips_at_the_period_it_names() {
        assert_eq!(mark(3599, HOUR), "~");
        assert_eq!(mark(3600, HOUR), "");
        assert_eq!(mark(86_399, DAY), "~");
        assert_eq!(mark(86_400, DAY), "");
    }

    /// An age that covers the hour but not the day marks the day ALONE:
    /// the two values are judged against their own periods, not against
    /// one verdict for the badge.
    #[test]
    fn a_covered_period_drops_its_mark() {
        let tp = Throughput {
            active_seconds: 7_200,
            ..sample_tp()
        };
        let out = human(&tp, &RateConfig::default(), false);
        assert_eq!(out, "3k,46k/total,13k/h,~280k/d\n");
    }

    /// A full day of sample marks nothing -- the badge only tildes what
    /// it had to extrapolate.
    #[test]
    fn a_day_long_sample_marks_nothing() {
        let tp = Throughput {
            active_seconds: 90_000,
            ..sample_tp()
        };
        let out = human(&tp, &RateConfig::default(), false);
        assert_eq!(out, "3k,46k/total,13k/h,280k/d\n");
    }

    /// The mark rides INSIDE the colored span: a projected value keeps
    /// its threshold color instead of trading one signal for the other.
    #[test]
    fn a_marked_value_keeps_its_color() {
        let config = RateConfig {
            hour: Some(Threshold {
                green: 1_000,
                amber: 100_000,
            }),
            ..RateConfig::default()
        };
        let out = human(&sample_tp(), &config, true);
        assert!(out.contains("\x1b[33m~13k/h\x1b[0m"), "{out}");
    }

    fn sample_tp() -> Throughput {
        Throughput {
            last_turn: 2_117,
            total: 45_305,
            per_hour: Some(12_400),
            per_day: Some(280_000),
            turns: 22,
            age_seconds: 3120,
            active_seconds: 3120,
        }
    }

    #[test]
    fn json_is_one_line() {
        let out = json(&sample_tp());
        assert_eq!(out.lines().count(), 1);
        assert!(!out.contains('~'), "no tilde in json (V9)");
    }

    #[test]
    fn json_carries_all_fields() {
        let out = json(&sample_tp());
        assert!(out.contains("\"last_turn\":2117"));
        assert!(out.contains("\"total\":45305"));
        assert!(out.contains("\"per_hour\":12400"));
        assert!(out.contains("\"per_day\":280000"));
    }

    /// V110 in json is a boolean per metric (V9), and it tracks the same
    /// boundary the tilde does.
    #[test]
    fn json_flags_each_projection() {
        let short = json(&sample_tp());
        assert!(short.contains("\"projected_hour\":true"), "{short}");
        assert!(short.contains("\"projected_day\":true"), "{short}");
        let long = json(&Throughput {
            active_seconds: 90_000,
            ..sample_tp()
        });
        assert!(long.contains("\"projected_hour\":false"), "{long}");
        assert!(long.contains("\"projected_day\":false"), "{long}");
    }

    #[test]
    fn color_applied_when_threshold_present() {
        let th = Some(Threshold {
            green: 50_000,
            amber: 100_000,
        });
        let green = colored("1k".to_owned(), 1_000, th, true);
        assert!(green.contains("\x1b[32m"), "green: {green}");
        let amber = colored("60k".to_owned(), 60_000, th, true);
        assert!(amber.contains("\x1b[33m"), "amber: {amber}");
        let red = colored("120k".to_owned(), 120_000, th, true);
        assert!(red.contains("\x1b[31m"), "red: {red}");
    }

    #[test]
    fn color_absent_without_threshold() {
        let out = colored("1k".to_owned(), 1_000, None, true);
        assert!(!out.contains('\x1b'), "no ANSI without threshold");
    }

    /// `auto` follows STDOUT, which is where the badge is printed.
    ///
    /// Under the test harness stdout is captured, never a terminal, so the
    /// verdict is a known false -- and that is the direction that matters:
    /// escapes must not land in a pipe or a file. The probe used to read
    /// stderr, which is a different stream with a different answer, and
    /// nothing asserted the verdict at all (only that the branch ran).
    #[test]
    fn auto_reads_the_stream_the_badge_is_written_to() {
        let piped = !std::io::IsTerminal::is_terminal(&std::io::stdout());
        assert!(piped, "the harness captures stdout");
        assert!(!want_color(ColorMode::Auto), "no escapes into a pipe");
        assert!(want_color(ColorMode::Always));
        assert!(!want_color(ColorMode::Never));
    }

    #[test]
    fn color_absent_when_disabled() {
        let th = Some(Threshold {
            green: 50_000,
            amber: 100_000,
        });
        let out = colored("1k".to_owned(), 1_000, th, false);
        assert!(!out.contains('\x1b'), "no ANSI when color=false");
    }

    #[test]
    fn toml_config_parses() {
        let text = r#"
[rate]
turn = { green = 50000, amber = 100000 }
total = { green = 1000000, amber = 5000000 }
"#;
        let parsed: TomlConfig = basic_toml::from_str(text).unwrap_or_default();
        let rate = parsed.rate.unwrap_or_default();
        assert_eq!(rate.turn.as_ref().map(|t| t.green), Some(50000));
        assert_eq!(rate.turn.as_ref().map(|t| t.amber), Some(100000));
        assert_eq!(rate.total.as_ref().map(|t| t.green), Some(1000000));
        assert!(rate.hour.is_none());
    }

    #[test]
    fn malformed_toml_degrades_silently() {
        let text = "this is not valid toml [[[";
        let parsed: Result<TomlConfig, _> = basic_toml::from_str(text);
        assert!(parsed.is_err());
    }

    /// The whole verb over a real one-turn transcript, so `run`'s short
    /// session branch is exercised by a PLANTED file rather than by
    /// whichever transcripts the machine running coverage happens to hold
    /// (B3, and B27's spread).
    #[test]
    fn a_one_turn_transcript_answers_in_both_formats() {
        let path = fixture("minimal.jsonl");
        let human = super::rate(std::slice::from_ref(&path), &nothing);
        assert!(human.out.is_empty(), "badge hides: {:?}", human.out);
        assert_eq!(human.code, 0);
        let js = super::rate(
            &[path, "--format".to_owned(), "json".to_owned()],
            &nothing,
        );
        assert!(js.out.starts_with('{'), "one object: {:?}", js.out);
        assert!(js.out.contains("\"per_hour\":null"), "{}", js.out);
    }

    /// The two config fallbacks that are NOT a missing file: malformed
    /// TOML, and valid TOML with no `[rate]` table. Both degrade to the
    /// default silently, because colour is cosmetic and a broken config
    /// must not break the measurement (V109).
    #[test]
    fn a_broken_or_rateless_config_degrades_to_the_default() {
        let broken = parse_config("[rate\nturn = oops");
        assert!(broken.turn.is_none() && broken.hour.is_none());
        let rateless = parse_config("[limits]\nsrc = 1000\n");
        assert!(rateless.turn.is_none() && rateless.day.is_none());
    }

    /// `rate` has no tokenizer tiers to pick from, so the flags that name
    /// one are a usage error naming the verb (V64's shared message).
    #[test]
    fn tier_flags_are_rejected_by_name() {
        for flag in ["--bpe", "--ollama"] {
            let out = super::rate(&[flag.to_owned()], &nothing);
            assert_eq!(out.code, 2, "{flag} is a usage error");
            assert!(out.err.contains("rate"), "names the verb: {}", out.err);
        }
    }

    fn fixture(name: &str) -> String {
        format!(
            "{}/tests/fixtures/session/{name}",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    fn nothing() -> String {
        String::new()
    }

    /// Every test below drives the verb through an input source of its
    /// own. Shadowing the real `rate` keeps that from being something
    /// each test has to remember.
    fn rate(args: &[String]) -> Output {
        super::rate(args, &nothing)
    }

    fn payload(transcript: &str, cwd: &str) -> String {
        format!(
            "{{\"session_id\":\"s1\",\"transcript_path\":\"{transcript}\",\
             \"cwd\":\"{cwd}\",\"model\":{{\"id\":\"claude\"}}}}"
        )
    }

    /// The payload names the transcript, and the badge reports THAT one.
    #[test]
    fn statusline_reads_the_transcript_from_the_payload() {
        let text = payload(&fixture("tool-shapes.jsonl"), "/tmp");
        let src = || text.clone();
        let out = super::rate(&["--statusline".to_owned()], &src);
        assert_eq!(out.code, 0);
        assert!(out.out.starts_with("(itok:"), "wrapped: {:?}", out.out);
        assert!(out.out.ends_with(')'), "wrapped: {:?}", out.out);
    }

    /// The harness captures the string, so there is no tty to detect and
    /// `auto` would strip every badge's color. Default is `always`.
    #[test]
    fn statusline_colors_by_default() {
        let dir = std::env::temp_dir().join("itok-rate-statusline");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(
            dir.join("itok.toml"),
            "[rate]\nturn = { green = 1, amber = 2 }\n",
        );
        let text =
            payload(&fixture("tool-shapes.jsonl"), &dir.display().to_string());
        let src = || text.clone();
        let out = super::rate(&["--statusline".to_owned()], &src);
        assert!(out.out.contains('\x1b'), "ANSI by default: {:?}", out.out);
    }

    /// An explicit `--color` still wins -- the flag supplies a DEFAULT,
    /// it does not seize the setting.
    #[test]
    fn an_explicit_color_beats_the_statusline_default() {
        let text = payload(&fixture("tool-shapes.jsonl"), "/tmp");
        let src = || text.clone();
        let args = ["--statusline", "--color", "never"];
        let out = super::rate(
            &args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(),
            &src,
        );
        assert!(!out.out.contains('\x1b'), "explicit never: {:?}", out.out);
    }

    /// A payload with no transcript is a usage error, not a guess: the
    /// whole point of the flag is that the session is NAMED (V104).
    #[test]
    fn a_payload_without_a_transcript_is_a_usage_error() {
        let src = || "{\"session_id\":\"s1\"}".to_owned();
        let out = super::rate(&["--statusline".to_owned()], &src);
        assert_eq!(out.code, 2);
        assert!(out.err.contains("transcript_path"), "{:?}", out.err);
        assert!(out.out.is_empty(), "statusline renders nothing");
    }

    /// The guarantee that keeps a bare `itok rate` from blocking on a
    /// terminal: without the flag, the input source is never touched.
    #[test]
    fn stdin_is_read_only_under_the_flag() {
        let seen = std::cell::Cell::new(false);
        let src = || {
            seen.set(true);
            String::new()
        };
        let _ = super::rate(&[fixture("tool-shapes.jsonl")], &src);
        assert!(!seen.get(), "bare rate must not read stdin");
        let _ = super::rate(&["--statusline".to_owned()], &src);
        assert!(seen.get(), "--statusline must read stdin");
    }

    /// Nothing to report hides the badge entirely -- `(itok:)` wrapped
    /// around nothing is a widget announcing its own silence (V3).
    #[test]
    fn an_empty_rate_produces_no_badge() {
        assert_eq!(badge(String::new()), "");
        assert_eq!(badge("\n".to_owned()), "");
        assert_eq!(badge("1k,2k/total\n".to_owned()), "(itok:1k,2k/total)");
    }

    #[test]
    fn rate_on_fixture_exits_zero() {
        let out = rate(&[fixture("tool-shapes.jsonl")]);
        assert_eq!(out.code, 0, "report-only (V5)");
    }

    #[test]
    fn rate_json_on_fixture() {
        let out = rate(&[
            fixture("tool-shapes.jsonl"),
            "--format".to_owned(),
            "json".to_owned(),
        ]);
        assert_eq!(out.code, 0);
        assert!(out.out.contains("\"last_turn\":"));
        assert!(out.out.contains("\"turns\":"));
    }

    #[test]
    fn rate_with_color_never() {
        let out = rate(&[
            fixture("tool-shapes.jsonl"),
            "--color".to_owned(),
            "never".to_owned(),
        ]);
        assert_eq!(out.code, 0);
        assert!(!out.out.contains('\x1b'), "no ANSI with --color never");
    }

    #[test]
    fn a_named_miss_is_a_usage_error() {
        let out = rate(&["/nonexistent/s.jsonl".to_owned()]);
        assert_eq!(out.code, 2);
    }

    #[test]
    fn unknown_flags_are_usage_errors() {
        assert_eq!(rate(&["--nope".to_owned()]).code, 2);
        assert_eq!(rate(&["--color".to_owned(), "rainbow".to_owned()]).code, 2);
        assert_eq!(rate(&["--format".to_owned(), "yaml".to_owned()]).code, 2);
    }

    /// Both clocks are in json: the span the session covered, and the
    /// working time that divides (V111). A reader who wants to know why
    /// a rate looks high can see the two apart.
    #[test]
    fn json_carries_both_clocks() {
        let out = json(&Throughput {
            age_seconds: 300_000,
            active_seconds: 3_120,
            ..sample_tp()
        });
        assert!(out.contains("\"age_seconds\":300000"), "{out}");
        assert!(out.contains("\"active_seconds\":3120"), "{out}");
    }
}
