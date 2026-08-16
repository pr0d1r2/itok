//! The `rate` command: pre-formatted throughput string for a statusline
//! badge (V46). Report-only, exit 0 (V5). Aggregates, never verdicts
//! (V59). Per-metric ANSI color from `itok.toml` (V109).

use crate::args::Format;
use crate::cli::Output;
use crate::session::Session;
use crate::tracecmd::{value, Origin};

#[derive(Default)]
struct Raw {
    session: Option<String>,
    color: ColorMode,
    format: Format,
    chdir: Option<String>,
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
    per_hour: u64,
    per_day: u64,
    turns: usize,
    age_seconds: u64,
}

pub(crate) fn rate(rest: &[String]) -> Output {
    match parse(rest) {
        Ok(raw) => run(&raw),
        Err(e) => Output::usage_err(format!("itok: {e}")),
    }
}

fn run(raw: &Raw) -> Output {
    let (session, origin) = match session_of(raw) {
        Ok(v) => v,
        Err(o) => return o,
    };
    let Some(tp) = throughput(&session) else {
        return Output::ok(String::new());
    };
    format_output(raw, &tp, &origin)
}

fn format_output(raw: &Raw, tp: &Throughput, origin: &Origin) -> Output {
    let note = crate::tracecmd::origin_note(origin);
    let config = load_config(raw.chdir.as_deref());
    let want_color = want_color(raw.color);
    Output::ok(match raw.format {
        Format::Json => json(tp),
        Format::Human => human(tp, &config, want_color) + &note,
    })
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
    let total = session.billed_input();
    let age = age_seconds(&first.ts, &last.ts);
    tp_from(
        last.billed_input().unwrap_or(0),
        total,
        age,
        session.turns.len(),
    )
}

fn tp_from(
    last_turn: u64,
    total: u64,
    age: u64,
    turns: usize,
) -> Option<Throughput> {
    let age_clamped = if age == 0 { 1 } else { age };
    Some(Throughput {
        last_turn,
        total,
        per_hour: extrapolate(total, age_clamped, 3600),
        per_day: extrapolate(total, age_clamped, 86400),
        turns,
        age_seconds: age,
    })
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

fn parse_ts(ts: &str) -> u64 {
    let s = ts.get(..10).unwrap_or(ts);
    s.parse::<u64>().unwrap_or(0)
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
    let parts = [
        metric(tp.last_turn, "", config.turn, color),
        metric(tp.total, "/total", config.total, color),
        metric(tp.per_hour, "/h", config.hour, color),
        metric(tp.per_day, "/d", config.day, color),
    ];
    format!("{}\n", parts.join(","))
}

fn metric(
    value: u64,
    suffix: &str,
    threshold: Option<Threshold>,
    color: bool,
) -> String {
    let text = format!("{}{suffix}", shorten(value));
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

fn json(tp: &Throughput) -> String {
    format!(
        "{{\"last_turn\":{},\"total\":{},\"per_hour\":{},\
         \"per_day\":{},\"turns\":{},\"age_seconds\":{}}}\n",
        tp.last_turn,
        tp.total,
        tp.per_hour,
        tp.per_day,
        tp.turns,
        tp.age_seconds,
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

fn is_tty() -> bool {
    std::io::IsTerminal::is_terminal(&std::io::stderr())
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
            return Err(crate::tracecmd::no_real_tier(a, "rate"))
        }
        "--color" => raw.color = parse_color(&value(it, a)?)?,
        "--format" => raw.format = crate::tracecmd::format_of(&value(it, a)?)?,
        "-C" => raw.chdir = Some(value(it, a)?),
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
        per_hour: 0,
        per_day: 0,
        turns: 0,
        age_seconds: 0,
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

    #[test]
    fn throughput_needs_at_least_two_turns() {
        let s = session_with_turns(&[(1000, "1000000000")]);
        assert!(throughput(&s).is_none());
        let empty = Session::default();
        assert!(throughput(&empty).is_none());
    }

    #[test]
    fn throughput_computes_totals() {
        let s =
            session_with_turns(&[(1_000, "1000000000"), (2_000, "1000003600")]);
        let tp = throughput(&s);
        assert!(tp.is_some());
        let tp = tp.unwrap_or(ZERO_TP);
        assert_eq!(tp.last_turn, 2_000);
        assert_eq!(tp.total, 3_000);
        assert_eq!(tp.turns, 2);
    }

    #[test]
    fn throughput_computes_wallclock_rates() {
        let s =
            session_with_turns(&[(1_000, "1000000000"), (2_000, "1000003600")]);
        let tp = throughput(&s).unwrap_or(ZERO_TP);
        assert_eq!(tp.age_seconds, 3600);
        assert_eq!(tp.per_hour, 3_000);
        assert_eq!(tp.per_day, 72_000);
    }

    #[test]
    fn human_output_shape() {
        let out = human(&sample_tp(), &RateConfig::default(), false);
        assert_eq!(out, "3k,46k/total,13k/h,280k/d\n");
    }

    fn sample_tp() -> Throughput {
        Throughput {
            last_turn: 2_117,
            total: 45_305,
            per_hour: 12_400,
            per_day: 280_000,
            turns: 22,
            age_seconds: 3120,
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
    fn parse_ts_extracts_epoch_seconds() {
        assert_eq!(parse_ts("1000000000.123"), 1000000000);
        assert_eq!(parse_ts("1718000000"), 1718000000);
        assert_eq!(parse_ts(""), 0);
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

    fn fixture(name: &str) -> String {
        format!(
            "{}/tests/fixtures/session/{name}",
            env!("CARGO_MANIFEST_DIR")
        )
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
}
