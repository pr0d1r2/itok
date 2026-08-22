//! The `headroom` command: `df` for a context (V91).
//!
//! `estimate` is `du` (V8); `df` is `du`'s sibling and the gap was real --
//! "how much room is LEFT" had no verb. The columns are df's own (window,
//! used, avail, use%) so the layout needs no teaching (V1).
//!
//! The rate keeps loadavg's THREE-WINDOW shape, because comparing a short
//! window against a long one reads the TREND at a glance. But the CLOCK
//! is TURNS, not wall-seconds (V91): context grows per turn and nothing
//! happens between them, so a per-second rate would report "load
//! dropping" through an idle hour while the window sat exactly as full as
//! it was. V2's rule -- keep the convention's SEMANTICS (work per unit of
//! the clock that matters), not its literal unit.
//!
//! NAMING TRAP, guarded by hand: `window` means two different things
//! here. `Session::window()` is what the model RECEIVED on the last turn
//! -- that is `used`. The df `window` column is CAPACITY, which comes
//! from `--window`, `--model`, or `.context-models`. This module never
//! reuses one name for both; they are `capacity` and `used` throughout.
//!
//! No capacity means NO denominator (V92): `avail`, `use%` and `turns
//! left` are all absent rather than computed against a guessed 200k or
//! 1M, which would make every derived column a fiction while looking
//! measured.
//!
//! Report-only, exit 0 (V5). The columns are arithmetic; whether the
//! number is healthy is judgment, and judgment belongs to `doctor` (V59).

use crate::args::Format;
use crate::cli::Output;
use crate::render::human;
use crate::session::Session;
use crate::tracecmd::{Origin, value};

/// The rate windows, in turns -- loadavg's three, on a turn clock (V91).
/// Ordered shortest-first, which is also the order `turns left` prefers.
const WINDOWS: [usize; 3] = [10, 50, 200];

#[derive(Default)]
struct Raw {
    session: Option<String>,
    model: Option<String>,
    window: Option<String>,
    human: bool,
    format: Format,
    chdir: Option<String>,
    task: Option<String>,
}

/// One `df` row: capacity, occupancy, and the rate triple.
struct Room {
    /// The context's capacity. `None` when nothing named one (V92).
    capacity: Option<u64>,
    /// Current occupancy -- exact, from the harness's own usage record.
    used: u64,
    rates: [Option<u64>; 3],
    /// What one task costs, DECLARED by the caller (T86). `None` until a
    /// project has enough single-task sessions to derive it -- deriving it
    /// from two would be V95's anecdote.
    task: Option<u64>,
}

pub(crate) fn headroom(rest: &[String]) -> Output {
    match parse(rest) {
        Ok(raw) => run(&raw),
        Err(e) => Output::usage_err(format!("itok: {e}")),
    }
}

fn run(raw: &Raw) -> Output {
    match (capacity(raw), task_cost(raw)) {
        (Ok(cap), Ok(task)) => report(raw, cap, task),
        (Err(e), _) | (_, Err(e)) => Output::usage_err(format!("itok: {e}")),
    }
}

/// Nothing to read is not an error (V5), and no usage anywhere means the
/// whole row is absent rather than zero -- a zero would read as a
/// measurement of an empty context (V47).
fn report(raw: &Raw, cap: Option<u64>, task: Option<u64>) -> Output {
    let (parsed, origin) = match session_of(raw) {
        Ok(v) => v,
        Err(o) => return o,
    };
    let note = crate::tracecmd::origin_note(&origin);
    let Some(room) = room_of(&parsed, cap, task) else {
        return Output::ok(String::new());
    };
    Output::ok(match raw.format {
        Format::Json => json(&room),
        Format::Human => table(&room, raw.human) + &note,
    })
}

/// The row, or `None` when no turn reported usage -- absent, not zero (V47).
fn room_of(
    parsed: &Session,
    cap: Option<u64>,
    task: Option<u64>,
) -> Option<Room> {
    Some(Room {
        capacity: cap,
        used: parsed.window()?,
        rates: WINDOWS.map(|n| parsed.rate(n)),
        task,
    })
}

/// The session, and WHERE it came from -- the origin is part of the claim
/// this verb makes about whose context it is reporting (V96/T74).
fn session_of(raw: &Raw) -> Result<(Session, Origin), Output> {
    crate::tracecmd::session_at(raw.session.as_deref(), raw.chdir.as_deref())
}

/// One task's declared cost, in the one unit grammar (V18).
fn task_cost(raw: &Raw) -> Result<Option<u64>, String> {
    match raw.task.as_deref() {
        Some(t) => crate::units::parse(t).map(Some),
        None => Ok(None),
    }
}

/// Capacity from `--window`, else `--model` via `.context-models`.
/// Explicit `--window` WINS over the table (V18), and an unknown model is
/// an error rather than a default, so a wrong capacity can never be
/// silently assumed (V11).
fn capacity(raw: &Raw) -> Result<Option<u64>, String> {
    if let Some(w) = raw.window.as_deref() {
        return crate::units::parse(w).map(Some);
    }
    let root = std::path::PathBuf::from(raw.chdir.as_deref().unwrap_or("."));
    Ok(crate::models::tier(raw.model.as_deref(), &root)?.window)
}

impl Room {
    /// Room left. `None` without a capacity (V92).
    fn avail(&self) -> Option<u64> {
        Some(self.capacity?.saturating_sub(self.used))
    }

    /// Percent full. A zero capacity yields `None`: it is not a capacity,
    /// and dividing by it would be a crash dressed as a column.
    fn pct(&self) -> Option<u64> {
        let cap = self.capacity.filter(|c| *c > 0)?;
        self.used.saturating_mul(100).checked_div(cap)
    }

    /// The most RECENT rate available, with the window it came from.
    /// df asks "how long at the CURRENT write rate", so the shortest
    /// window that has enough turns wins -- and it is reported, because
    /// the distribution is heavy-tailed and which window answered is part
    /// of what the number means (V93).
    fn current(&self) -> Option<(u64, usize)> {
        WINDOWS
            .iter()
            .zip(self.rates)
            .find_map(|(w, r)| Some((r?, *w)))
    }

    /// How many more tasks fit: `avail / task`.
    ///
    /// The whole can-I-finish question reduces to `>= 1`. Arithmetic only,
    /// so it stays inside V59's boundary -- the VERDICT ("start fresh
    /// instead") belongs to `doctor --session`, which V99 binds to V97's two
    /// honest levers.
    ///
    /// `None` without a capacity (V92, no denominator) or without a declared
    /// cost: a task size guessed on the caller's behalf would make every
    /// answer a fiction that looks measured.
    fn tasks_left(&self) -> Option<u64> {
        self.avail()?.checked_div(self.task?)
    }

    /// Turns until full at that rate -- an EXTRAPOLATION (V93), never a
    /// verdict. A rate of zero yields `None`: "never fills" is a claim,
    /// and infinity is not a column.
    fn turns_left(&self) -> Option<(u64, usize)> {
        let (rate, window) = self.current()?;
        let left = self.avail()?.checked_div(rate)?;
        Some((left, window))
    }
}

fn size(n: u64, h: bool) -> String {
    if h { human(n) } else { n.to_string() }
}

/// An absent number is a dash, never a zero (V47/V92).
fn cell(v: Option<u64>, h: bool) -> String {
    v.map_or_else(|| "-".to_owned(), |n| size(n, h))
}

fn triple(rates: &[Option<u64>; 3], h: bool) -> String {
    rates
        .iter()
        .map(|r| cell(*r, h))
        .collect::<Vec<_>>()
        .join("/")
}

/// `turns left` is the one extrapolated column, so it is the one that
/// carries the tilde (V3/V93).
fn left_cell(room: &Room, h: bool) -> String {
    room.turns_left()
        .map_or_else(|| "-".to_owned(), |(n, _)| format!("~{}", size(n, h)))
}

const HEADER: &str =
    "  window      used     avail  use%   rate 10/50/200      turns left";

/// The `tasks left` column exists only when a cost was DECLARED. An
/// always-present column reading `-` would invite the reader to supply a
/// number from nowhere, which is the guess V92 forbids.
fn task_header(room: &Room) -> &'static str {
    if room.task.is_some() {
        "  tasks left\n"
    } else {
        "\n"
    }
}

/// `tasks left`, tilde'd: it divides an exact remainder by a DECLARED cost,
/// so it is only as true as the declaration (V3).
fn tasks_cell(room: &Room, h: bool) -> String {
    match room.tasks_left() {
        Some(n) => format!("  {:>10}", format!("~{}", size(n, h))),
        None if room.task.is_some() => "           -".to_owned(),
        None => String::new(),
    }
}

fn table(room: &Room, h: bool) -> String {
    format!(
        "{HEADER}{}{:>8} {:>9} {:>9} {:>5} {:>15} {:>15}{}\n",
        task_header(room),
        cell(room.capacity, h),
        size(room.used, h),
        cell(room.avail(), h),
        room.pct()
            .map_or_else(|| "-".to_owned(), |p| format!("{p}%")),
        triple(&room.rates, h),
        left_cell(room, h),
        tasks_cell(room, h),
    ) + &notes(room)
}

/// What the numbers mean and what they rest on.
///
/// The rate line names its CLOCK, because "per turn" is the whole
/// distinction V91 draws. The turns-left line names its ASSUMPTION and
/// the window that produced it (V93). The capacity line says the
/// denominator is missing rather than letting three dashes speak for
/// themselves (V92).
///
/// Statements of method only -- no advice about what to do, which is
/// judgment and belongs elsewhere (V59).
fn notes(room: &Room) -> String {
    let mut out = String::from(
        "  rate = itok/turn over the last 10/50/200 turns; used is actual\n",
    );
    if let Some((_, w)) = room.turns_left() {
        out.push_str(&format!("  turns left: at the recent {w}-turn rate\n"));
    }
    out.push_str(&task_note(room));
    if room.capacity.is_none() {
        out.push_str(
            "  window unknown -- pass --model X or --window N for \
             avail/use%/turns left\n",
        );
    }
    out
}

/// Names WHAT `--task` measures, because the obvious reading is wrong.
///
/// `tasks left` divides AVAIL, so the declared cost must be the window a
/// task OCCUPIES, not what it bills. Measured on this project the two are
/// ~78k and ~27M -- a factor of 350, because input is re-billed every turn
/// (V94). Plug the billing figure in and every answer is `~0`, which looks
/// like a measurement (V3).
fn task_note(room: &Room) -> String {
    match room.task {
        Some(n) => format!(
            "  tasks left: at {n} itok of WINDOW per task (declared, not \
             billed cost)\n"
        ),
        None => String::new(),
    }
}

/// One object (V9). Absent values are `null`, keeping the key: a reader
/// must be able to tell "not measured" from "measured as zero" (V47), and
/// `null` is json's own word for absent.
fn null_or(v: Option<u64>) -> String {
    v.map_or_else(|| "null".to_owned(), |n| n.to_string())
}

/// The `df` columns.
fn json_df(room: &Room) -> String {
    format!(
        "\"window\":{},\"used\":{},\"avail\":{},\"use_pct\":{}",
        null_or(room.capacity),
        room.used,
        null_or(room.avail()),
        null_or(room.pct()),
    )
}

/// The rate triple and the extrapolation it feeds. `turns_left_window`
/// travels beside `turns_left` because which window answered is part of
/// what the number means (V93).
fn json_rate(room: &Room) -> String {
    format!(
        "\"rate_10\":{},\"rate_50\":{},\"rate_200\":{},\
         \"turns_left\":{},\"turns_left_window\":{},\"task\":{},\"tasks_left\":{}",
        null_or(room.rates[0]),
        null_or(room.rates[1]),
        null_or(room.rates[2]),
        null_or(room.turns_left().map(|(n, _)| n)),
        null_or(room.turns_left().map(|(_, w)| w as u64)),
        null_or(room.task),
        null_or(room.tasks_left()),
    )
}

/// Units and methods, fixed for every row: `used` is read from the
/// harness's usage record while `turns left` is extrapolated, and V3
/// requires each to say which it is.
const METHODS: &str = "\"unit\":\"input_tokens\",\
     \"rate_unit\":\"input_tokens_per_turn\",\"used_method\":\"actual\",\
     \"turns_left_method\":\"extrapolated at the recent rate\"";

fn json(room: &Room) -> String {
    format!("{{{},{},{METHODS}}}\n", json_df(room), json_rate(room))
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
        "-h" => raw.human = true,
        "--bpe" | "--ollama" => {
            return Err(crate::tracecmd::no_real_tier(a, "headroom"));
        }
        _ => return with_value(a, it, raw),
    }
    Ok(())
}

fn with_value<'a>(
    a: &str,
    it: &mut impl Iterator<Item = &'a String>,
    raw: &mut Raw,
) -> Result<(), String> {
    match a {
        "--model" => raw.model = Some(value(it, a)?),
        "--window" => raw.window = Some(value(it, a)?),
        "--task" => raw.task = Some(value(it, a)?),
        "-C" => raw.chdir = Some(value(it, a)?),
        "--format" => raw.format = crate::tracecmd::format_of(&value(it, a)?)?,
        other if other.starts_with('-') => {
            return Err(format!("unknown flag {other}"));
        }
        other => raw.session = Some(other.to_owned()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Turn;

    fn fixture(name: &str) -> String {
        format!(
            "{}/tests/fixtures/session/{name}",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    fn run_on(name: &str, extra: &[&str]) -> Output {
        let mut args = vec![fixture(name)];
        args.extend(extra.iter().map(|s| (*s).to_owned()));
        headroom(&args)
    }

    /// A session whose per-turn occupancy is exactly `windows`.
    fn session_of(windows: &[u64]) -> Session {
        let mut s = Session::default();
        for w in windows {
            s.turns.push(Turn {
                cache_read: Some(*w),
                ..Turn::default()
            });
        }
        s
    }

    fn room_with_task(
        capacity: Option<u64>,
        windows: &[u64],
        task: Option<u64>,
    ) -> Room {
        let mut r = room(capacity, windows);
        r.task = task;
        r
    }

    fn room(capacity: Option<u64>, windows: &[u64]) -> Room {
        let s = session_of(windows);
        Room {
            capacity,
            used: s.window().unwrap_or(0),
            rates: WINDOWS.map(|n| s.rate(n)),
            task: None,
        }
    }

    /// T86: `tasks left` is `avail / task`, and the can-I-finish question
    /// is just `>= 1`.
    #[test]
    fn tasks_left_divides_the_remainder_by_a_declared_cost() {
        let r = room_with_task(Some(1_000), &[400], Some(200));
        assert_eq!(r.avail(), Some(600));
        assert_eq!(r.tasks_left(), Some(3));
        let full = room_with_task(Some(1_000), &[900], Some(200));
        assert_eq!(full.tasks_left(), Some(0), "no room for another task");
    }

    /// V92: no capacity means no denominator, so no answer -- and a task
    /// size is never invented on the caller's behalf.
    #[test]
    fn tasks_left_needs_both_a_capacity_and_a_declared_cost() {
        assert_eq!(room_with_task(None, &[400], Some(200)).tasks_left(), None);
        assert_eq!(
            room_with_task(Some(1_000), &[400], None).tasks_left(),
            None
        );
    }

    /// The column and its note appear ONLY when a cost was declared: an
    /// always-present `-` would invite a number from nowhere.
    #[test]
    fn the_column_appears_only_when_a_cost_is_declared() {
        let without = table(&room(Some(1_000), &[400]), false);
        assert!(!without.contains("tasks left"), "no column: {without}");
        let with =
            table(&room_with_task(Some(1_000), &[400], Some(200)), false);
        assert!(with.contains("tasks left"), "column present");
        assert!(with.contains("WINDOW per task"), "the unit is named");
        assert!(with.contains("not billed cost"), "and the trap is named");
    }

    /// V3: it divides an exact remainder by a DECLARED cost, so it is only
    /// as true as the declaration -- hence the tilde.
    #[test]
    fn tasks_left_carries_the_tilde() {
        let out = table(&room_with_task(Some(1_000), &[400], Some(200)), false);
        let row = out.lines().nth(1).unwrap_or_default();
        assert!(
            row.contains("~3"),
            "tilde on a declared-cost division: {row}"
        );
    }

    /// V91: df's own columns, so the layout needs no teaching.
    #[test]
    fn the_columns_are_dfs_own() {
        let out = run_on("minimal.jsonl", &["--window", "200k"]);
        for col in ["window", "used", "avail", "use%"] {
            assert!(out.out.contains(col), "missing df column `{col}`");
        }
        assert_eq!(out.code, 0, "report-only (V5)");
    }

    /// V91: the clock is TURNS, not wall-seconds. A per-second rate would
    /// report load dropping through an idle hour while the window sat
    /// exactly as full as it was.
    #[test]
    fn the_rate_is_per_turn_not_per_second() {
        let out = run_on("minimal.jsonl", &["--window", "200k"]);
        assert!(out.out.contains("itok/turn"));
        for wall in ["/s", "per second", "/sec"] {
            assert!(!out.out.contains(wall), "wall-clock unit: {wall}");
        }
    }

    /// V91: three windows, because the SPREAD is what reads the trend.
    #[test]
    fn the_rate_keeps_loadavgs_three_windows() {
        assert_eq!(WINDOWS, [10, 50, 200]);
        let out = run_on("minimal.jsonl", &[]);
        assert!(out.out.contains("10/50/200"));
    }

    /// V92, the one that matters: no capacity means NO denominator.
    /// Assuming 200k or 1M would make every derived column a fiction
    /// while looking measured.
    #[test]
    fn no_window_gives_no_denominator_and_says_so() {
        let out = run_on("minimal.jsonl", &[]);
        assert!(out.out.contains("window unknown"));
        for guess in ["200000", "1000000", "200k", "1M"] {
            assert!(!out.out.contains(guess), "guessed capacity: {guess}");
        }
    }

    /// V92: `used` still reports -- it needs no denominator.
    #[test]
    fn used_is_reported_without_a_capacity() {
        let r = room(None, &[100, 300]);
        assert_eq!(r.used, 300);
        assert_eq!(r.avail(), None);
        assert_eq!(r.pct(), None);
    }

    /// V18: explicit `--window` wins over the table.
    #[test]
    fn an_explicit_window_wins_over_the_model() {
        let raw = Raw {
            window: Some("50k".to_owned()),
            model: Some("nonexistent-model".to_owned()),
            ..Raw::default()
        };
        // The model would ERROR if consulted (V11), so 50k proves
        // precedence rather than merely agreeing with it.
        assert_eq!(capacity(&raw), Ok(Some(50_000)));
    }

    /// V93: the extrapolated column carries the tilde and names the
    /// assumption it rests on. Everything else is exact arithmetic.
    #[test]
    fn turns_left_is_marked_as_an_extrapolation() {
        let growing: Vec<u64> = (1..=12).map(|i| i * 1_000).collect();
        let r = room(Some(100_000), &growing);
        let out = table(&r, false);
        assert!(out.contains("at the recent"), "the assumption is named");
        let win = out.lines().nth(1).unwrap_or_default();
        assert!(win.contains('~'), "turns left carries the tilde");
    }

    /// V93's measured regression: one real turn reported a window of 0,
    /// and differencing it against its neighbours rendered a -387,741
    /// "compaction" that never happened. The zero turn is EXCLUDED, so
    /// the rate stays the honest growth of the turns that reported.
    #[test]
    fn a_zero_window_turn_is_excluded_from_the_rate() {
        let s = session_of(&[1_000, 0, 1_020]);
        assert_eq!(s.occupancy(), vec![1_000, 1_020], "the zero is dropped");
        // Two samples, one delta: the 10-turn window cannot be filled.
        assert_eq!(s.rate(1), Some(20), "growth between reporting turns");
        assert_eq!(s.rate(10), None);
    }

    /// The same zero, end to end: no absurd rate reaches the table.
    #[test]
    fn a_zero_window_never_produces_an_absurd_rate() {
        let mut occ = vec![1_000u64; 12];
        occ.push(0);
        occ.push(1_100);
        let s = session_of(&occ);
        let rate = s.rate(10).unwrap_or(u64::MAX);
        assert!(rate < 1_000, "a dropped zero cannot inflate the rate");
        assert_eq!(s.window(), Some(1_100), "used is still the last turn");
    }

    /// V47: a window with too few turns is ABSENT, not zero. Substituting
    /// the shorter window's value would read as a flat trend when the
    /// truth is "not enough turns yet".
    #[test]
    fn an_unfilled_window_is_absent_rather_than_zero() {
        let s = session_of(&[100, 200, 300]);
        assert_eq!(s.rate(2), Some(100));
        assert_eq!(s.rate(50), None, "not enough turns");
        assert_eq!(s.rate(200), None);
    }

    /// A shrinking context yields 0, never a negative -- `turns left`
    /// would otherwise extrapolate backwards.
    #[test]
    fn a_shrinking_context_rates_zero_not_negative() {
        let s = session_of(&[5_000, 4_000, 3_000]);
        assert_eq!(s.rate(2), Some(0));
    }

    /// A zero rate means no extrapolation: "never fills" is a claim, and
    /// infinity is not a column.
    #[test]
    fn a_zero_rate_gives_no_turns_left() {
        let r = room(Some(100_000), &[1_000; 12]);
        assert_eq!(r.turns_left(), None, "flat context does not fill");
        assert!(table(&r, false).contains('-'));
    }

    /// The dash is the absent marker everywhere, so a reader never sees a
    /// zero standing in for "not measured" (V47).
    #[test]
    fn absent_numbers_render_as_a_dash() {
        assert_eq!(cell(None, false), "-");
        assert_eq!(cell(Some(0), false), "0", "a measured zero still shows");
        assert_eq!(triple(&[None, None, None], false), "-/-/-");
    }

    /// V9: one object, stable keys, `null` for absent, no tilde in a
    /// numeric field.
    #[test]
    fn json_is_one_object_with_nulls_for_absent() {
        let out = run_on("minimal.jsonl", &["--format", "json"]);
        assert_eq!(out.out.lines().count(), 1);
        assert!(out.out.contains("\"window\":null"), "no capacity -> null");
        assert!(out.out.contains("\"avail\":null"));
        assert!(out.out.contains("\"unit\":\"input_tokens\""));
        assert!(out.out.contains("\"rate_unit\":\"input_tokens_per_turn\""));
        assert!(!out.out.contains('~'), "no tilde in json (V9)");
    }

    /// V3: the exact number and the extrapolated one are labelled apart.
    #[test]
    fn json_names_a_method_per_figure() {
        let out = run_on("minimal.jsonl", &["--format", "json"]);
        assert!(out.out.contains("\"used_method\":\"actual\""));
        assert!(out.out.contains("\"turns_left_method\":\"extrapolated"));
    }

    /// `-h` abbreviates, like `df -h`.
    #[test]
    fn human_sizes_abbreviate() {
        assert_eq!(size(200_000, true), "200k");
        assert_eq!(size(200_000, false), "200000");
    }

    /// V59: arithmetic and method, never advice. Judgment belongs to
    /// `doctor`.
    #[test]
    fn the_report_gives_no_advice() {
        let r = room(Some(100_000), &[1_000, 2_000, 3_000]);
        let out = table(&r, false).to_lowercase();
        for word in ["should", "consider", "unhealthy", "too much", "reduce"] {
            assert!(!out.contains(word), "no advice: {word}");
        }
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
        let out = headroom(&["/nonexistent/s.jsonl".to_owned()]);
        assert_eq!(out.code, 2, "a name that resolves to nothing");
        assert!(out.out.is_empty(), "nothing on stdout");
        assert!(out.err.contains("no session"), "{}", out.err);
    }

    /// V45: a real tier is refused with its reason -- no content is
    /// stored, so there is nothing for a tokenizer to count.
    #[test]
    fn a_real_tier_is_refused() {
        let out = run_on("minimal.jsonl", &["--bpe"]);
        assert_eq!(out.code, 2);
        assert!(out.err.contains("no content is stored"));
        assert!(out.err.contains("headroom"), "the message names the verb");
    }

    /// V11: an unknown model FAILS rather than defaulting to a capacity.
    #[test]
    fn an_unknown_model_is_an_error_not_a_default() {
        let out = run_on("minimal.jsonl", &["--model", "no-such-model"]);
        assert_eq!(out.code, 2);
    }

    #[test]
    fn unknown_flags_and_bad_values_are_usage_errors() {
        assert_eq!(run_on("minimal.jsonl", &["--nope"]).code, 2);
        assert_eq!(run_on("minimal.jsonl", &["--format", "yaml"]).code, 2);
        assert_eq!(run_on("minimal.jsonl", &["--window"]).code, 2);
        assert_eq!(run_on("minimal.jsonl", &["--window", "eleventy"]).code, 2);
    }

    /// use% is exact arithmetic over two exact numbers.
    #[test]
    fn use_percent_is_the_ratio() {
        let r = room(Some(1_000), &[610]);
        assert_eq!(r.pct(), Some(61));
        assert_eq!(r.avail(), Some(390));
    }

    /// A zero capacity is not a capacity: dividing by it would be a crash
    /// dressed as a column.
    #[test]
    fn a_zero_capacity_yields_no_percentage() {
        let r = room(Some(0), &[100]);
        assert_eq!(r.pct(), None);
    }

    /// Occupancy over capacity is reported honestly: avail clamps at
    /// zero, but use% is allowed past 100 rather than being capped into a
    /// comfortable-looking number.
    #[test]
    fn an_overfull_context_clamps_avail_but_not_the_percentage() {
        let r = room(Some(1_000), &[1_500]);
        assert_eq!(r.avail(), Some(0));
        assert_eq!(r.pct(), Some(150));
    }
}
