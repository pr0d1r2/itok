//! T37: run the runtime verbs over itok's OWN dev sessions and report the
//! measured waste -- so M6's fuse thresholds come from data rather than
//! from a guess (V42).
//!
//! V42 is an ORDERING law: telemetry ships first and is usable alone,
//! enforcement ships last and is tuned from measured numbers, because a
//! threshold guessed without data has false-positives that cost more than
//! the waste it prevents. This file is the measuring instrument for that
//! step, and it deliberately sets no threshold of its own.
//!
//! REPORTS, never judges (V59). Every number here is arithmetic over what
//! the transcript recorded. "This is unhealthy, do X" is judgment and
//! belongs to `doctor --session`, which V99 binds to V97's two levers.
//!
//! Reads the verbs' JSON, not their tables (V9), and shells out to the
//! built binary rather than reaching into `topcmd`'s private aggregation:
//! one definition of every column, and this dogfoods the contract agents
//! actually parse (V15/V64).
//!
//! `#[ignore]` for the session pass: it reads `$HOME` and whatever
//! sessions happen to exist, so it can neither be hermetic nor
//! reproducible in the gate (V68). The gate still COMPILES it, so it
//! cannot rot, and the arithmetic below is unit-tested against synthetic
//! input that needs no transcript at all -- the shape `live_ollama_smoke`
//! and `derive_report` already use (V38).
//!
//!   cargo test --test dogfood_sessions -- --ignored --nocapture

use std::collections::BTreeMap;
use std::process::Command;

const DIR: &str = env!("CARGO_MANIFEST_DIR");

// ------------------------------------------------------------------ json

/// One numeric field, or `None` when absent or `null`.
///
/// Hand-rolled because `serde_json` is an OPTIONAL dependency behind a
/// feature (section C: the core carries zero required deps), and a test that
/// forced it on would make the dev build differ from the shipped one.
/// The shape read here is one flat object per line, which is exactly what
/// V9 promises, so a full parser would buy nothing.
fn num(line: &str, key: &str) -> Option<u64> {
    let at = line.find(&format!("\"{key}\":"))?;
    let tail = line.get(at.checked_add(key.len().checked_add(3)?)?..)?;
    let digits: String =
        tail.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// One string field. `null` reads as `None` -- absent is not empty (V47).
fn text(line: &str, key: &str) -> Option<String> {
    let at = line.find(&format!("\"{key}\":"))?;
    let tail = line.get(at.checked_add(key.len().checked_add(3)?)?..)?;
    let inner = tail.strip_prefix('"')?;
    let end = inner.find('"')?;
    inner.get(..end).map(str::to_owned)
}

// ------------------------------------------------------- the aggregates

/// One load, as `trace` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Load {
    ts: String,
    what: String,
    tokens: u64,
}

/// What repeated loading cost, over and above entering the context once.
#[derive(Debug, Default, PartialEq, Eq)]
struct Repeat {
    /// Every load, summed.
    total: u64,
    /// The FIRST load of each identity -- the unavoidable entry cost.
    entry: u64,
    /// Everything after the first, per identity.
    repeat: u64,
    /// Identities loaded more than once.
    repeated: usize,
    /// Who repeated, biggest repeat cost first. WHO matters as much as how
    /// much: a fuse aimed at the wrong class of load is worse than none.
    by_who: Vec<(String, u64)>,
}

/// Charge the first load of each identity to entry and the rest to repeat.
///
/// An UPPER BOUND on waste, and saying so is the point (V44/V3): `trace`
/// reports no content hash, so a genuinely CHANGED file re-read after an
/// edit counts here exactly like a wasteful re-read of identical bytes.
/// The honest claim is "this much was spent loading things that had been
/// loaded before", not "this much was wasted".
fn repeat_cost(loads: &[Load]) -> Repeat {
    let mut seen: BTreeMap<&str, u64> = BTreeMap::new();
    let mut by: BTreeMap<String, u64> = BTreeMap::new();
    let mut out = Repeat::default();
    for l in ordered(loads) {
        out.total = out.total.saturating_add(l.tokens);
        let n = seen.entry(l.what.as_str()).or_default();
        *n = n.saturating_add(1);
        charge(&mut out, &mut by, l, *n == 1);
    }
    out.repeated = seen.values().filter(|n| **n > 1).count();
    out.by_who = ranked(&by);
    out
}

/// First sighting is entry cost; every later one is repeat, and the
/// contributor is remembered so the total can be attributed.
fn charge(
    out: &mut Repeat,
    by: &mut BTreeMap<String, u64>,
    l: &Load,
    first: bool,
) {
    if first {
        out.entry = out.entry.saturating_add(l.tokens);
        return;
    }
    out.repeat = out.repeat.saturating_add(l.tokens);
    let acc = by.entry(l.what.clone()).or_default();
    *acc = acc.saturating_add(l.tokens);
}

/// Biggest repeat cost first, so the report names the contributor rather
/// than leaving a percentage to be read as "the agent re-reads files".
fn ranked(by: &BTreeMap<String, u64>) -> Vec<(String, u64)> {
    let mut v: Vec<(String, u64)> = by.clone().into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v
}

/// Chronological, because "first" is a claim about time and the caller may
/// hand these over in any order.
fn ordered(loads: &[Load]) -> Vec<&Load> {
    let mut v: Vec<&Load> = loads.iter().collect();
    v.sort_by(|a, b| a.ts.cmp(&b.ts));
    v
}

/// Percent, floored, or `None` without a denominator (V92).
fn pct(part: u64, whole: u64) -> Option<u64> {
    part.saturating_mul(100).checked_div(whole)
}

// ---------------------------------------------------------- the verbs

/// `itok <verb> <session> --format json`, as an agent would call it.
fn verb(args: &[&str]) -> Option<String> {
    let out = Command::new(env!("CARGO_BIN_EXE_itok"))
        .current_dir(DIR)
        .args(args)
        .args(["--format", "json"])
        .output()
        .ok()?;
    // A named session that cannot be read is a usage error now (V104), so
    // a non-zero code here is a real failure rather than an empty session.
    if !out.status.success() {
        println!("  ! {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Every session id recorded for THIS project.
///
/// Derived from the same `$HOME`/project rule the tool uses rather than
/// hardcoded, so the report follows the repo instead of one machine (V37).
fn session_ids() -> Vec<String> {
    let Some(dir) = project_dir() else {
        return Vec::new();
    };
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = rd
        .flatten()
        .filter_map(|e| stem_of_jsonl(&e.path()))
        .collect();
    ids.sort();
    ids
}

fn stem_of_jsonl(p: &std::path::Path) -> Option<String> {
    if p.extension()?.to_str()? != "jsonl" {
        return None;
    }
    Some(p.file_stem()?.to_str()?.to_owned())
}

/// `~/.claude/projects/<encoded cwd>`, the harness's own layout.
fn project_dir() -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let canon = std::fs::canonicalize(DIR).ok()?;
    let encoded: String = canon
        .to_str()?
        .chars()
        .map(|c| if c == '/' || c == '.' { '-' } else { c })
        .collect();
    Some(
        std::path::PathBuf::from(home)
            .join(".claude")
            .join("projects")
            .join(encoded),
    )
}

// ------------------------------------------------------- one session

/// What one session cost, and what repeated itself.
struct Measured {
    id: String,
    turns: u64,
    billed: u64,
    accounted: u64,
    unaccounted: u64,
    cold_turns: u64,
    cold_written: u64,
    carried: u64,
    repeat: Repeat,
}

fn measure(id: &str) -> Option<Measured> {
    let summary = summary_line(&verb(&["top", id])?)?;
    let loads = loads_of(&verb(&["trace", id])?);
    let carried = carried_total(&verb(&["top", id])?);
    Some(Measured {
        id: id.to_owned(),
        turns: num(&summary, "turns").unwrap_or(0),
        billed: num(&summary, "billed").unwrap_or(0),
        accounted: num(&summary, "accounted").unwrap_or(0),
        unaccounted: num(&summary, "unaccounted").unwrap_or(0),
        cold_turns: num(&summary, "cold_cache_turns").unwrap_or(0),
        cold_written: num(&summary, "cold_cache_written").unwrap_or(0),
        carried,
        repeat: repeat_cost(&loads),
    })
}

/// `top`'s last line is the ledger summary (V44's accounted-vs-unaccounted).
fn summary_line(json: &str) -> Option<String> {
    json.lines()
        .find(|l| l.contains("\"summary\":true"))
        .map(str::to_owned)
}

fn carried_total(json: &str) -> u64 {
    json.lines()
        .filter(|l| !l.contains("\"summary\":true"))
        .filter_map(|l| num(l, "carried"))
        .fold(0u64, |a, b| a.saturating_add(b))
}

fn loads_of(json: &str) -> Vec<Load> {
    json.lines().filter_map(load_of).collect()
}

/// A load's identity matches `top`'s: the path when one was named, else
/// the source in parens. One definition of "the same thing" (V64).
fn load_of(line: &str) -> Option<Load> {
    let ts = text(line, "ts")?;
    let what = text(line, "path").unwrap_or_else(|| {
        format!("({})", text(line, "source").unwrap_or_default())
    });
    Some(Load {
        ts,
        what,
        tokens: num(line, "tokens").unwrap_or(0),
    })
}

// ----------------------------------------------------------- the report

/// The dogfood pass (T37): every session this project recorded.
///
/// Prints `n` beside everything, because V95 is the rule this report exists
/// to respect -- two turns were once read as a behavioural finding and were
/// refuted by looking, so a number without its sample size is not evidence.
#[test]
#[ignore = "dogfood over real sessions; run with --ignored --nocapture"]
fn dogfood_report() {
    let ids = session_ids();
    if ids.is_empty() {
        println!("no sessions recorded for this project -- nothing measured");
        return;
    }
    let measured: Vec<Measured> =
        ids.iter().filter_map(|i| measure(i)).collect();
    println!("\n{} session(s) measured\n", measured.len());
    for m in &measured {
        print_session(m);
    }
    print_fleet(&measured);
}

fn print_session(m: &Measured) {
    println!("{}  n={} turns", m.id, m.turns);
    print_ledger(m);
    print_repeat(&m.repeat);
    print_carry(m);
}

/// What was billed, and how much of it itok can attribute (V44).
fn print_ledger(m: &Measured) {
    let seen = m.accounted.saturating_add(m.unaccounted);
    println!("   billed        {:>12}", m.billed);
    println!(
        "   accounted     {:>12}  ({}% of the window itok can attribute)",
        m.accounted,
        pct(m.accounted, seen).unwrap_or(0)
    );
}

/// The compounding cost, and the cold-cache observation (V95).
fn print_carry(m: &Measured) {
    println!(
        "   carried       {:>12}  (size x turns since entry)",
        m.carried
    );
    println!(
        "   cold cache    {:>12}  written over {} turn(s)",
        m.cold_written, m.cold_turns
    );
}

fn print_repeat(r: &Repeat) {
    println!(
        "   loads         {:>12}  entry, {} repeat ({}% of loading)",
        r.entry,
        r.repeat,
        pct(r.repeat, r.total).unwrap_or(0)
    );
    println!(
        "   repeated      {:>12}  identities loaded more than once",
        r.repeated
    );
    for (who, cost) in r.by_who.iter().take(3) {
        println!("     {cost:>10}  {who}");
    }
}

/// The fleet totals, and the one thing this report refuses to do.
fn print_fleet(all: &[Measured]) {
    let turns = sum(all, |m| m.turns);
    let repeat = sum(all, |m| m.repeat.repeat);
    let total = sum(all, |m| m.repeat.total);
    println!("\nFLEET  n={} sessions, {turns} turns", all.len());
    println!(
        "   repeat loading   {repeat} of {total} ({}%)",
        pct(repeat, total).unwrap_or(0)
    );
    println!(
        "   cold-cache turns {} of {turns}",
        sum(all, |m| m.cold_turns)
    );
    print_caveats();
}

fn sum(all: &[Measured], f: impl Fn(&Measured) -> u64) -> u64 {
    all.iter().fold(0u64, |a, m| a.saturating_add(f(m)))
}

/// What these numbers are NOT, printed with them rather than left to a
/// reader -- an aggregate handed over bare is what V95 was written about.
fn print_caveats() {
    println!(
        "\nUPPER BOUND: `trace` records no content hash, so a file re-read\n\
         after a real edit is counted here exactly like a wasteful re-read\n\
         of identical bytes. The claim is `spent loading things already\n\
         loaded`, not `wasted`.\n\
         NO THRESHOLD IS SET HERE (V42/V59): these are aggregates. Turning\n\
         one into a fuse tier is M6's decision, made against this `n`."
    );
}

// ------------------------------------------------------------- units

#[cfg(test)]
mod tests {
    use super::*;

    fn load(ts: &str, what: &str, tokens: u64) -> Load {
        Load {
            ts: ts.to_owned(),
            what: what.to_owned(),
            tokens,
        }
    }

    /// The first load of each identity is ENTRY; the rest are repeat.
    #[test]
    fn repeat_charges_everything_after_the_first_load() {
        let got = repeat_cost(&[
            load("1", "a.rs", 100),
            load("2", "b.rs", 10),
            load("3", "a.rs", 100),
            load("4", "a.rs", 100),
        ]);
        assert_eq!(got.total, 310);
        assert_eq!(got.entry, 110, "one a.rs and one b.rs");
        assert_eq!(got.repeat, 200, "two more a.rs");
        assert_eq!(got.repeated, 1, "only a.rs repeated");
    }

    /// PLANTED off-by-one: a session where nothing repeats must report ZERO
    /// repeat cost. Charging the first load would make every number in the
    /// report an overstatement, and an overstated waste figure is exactly
    /// what would set M6's fuse too tight (V42).
    #[test]
    fn a_single_load_of_each_thing_is_never_repeat_cost() {
        let got = repeat_cost(&[load("1", "a.rs", 100), load("2", "b.rs", 10)]);
        assert_eq!(got.repeat, 0, "nothing was loaded twice");
        assert_eq!(got.repeated, 0);
        assert_eq!(got.entry, got.total, "all of it is entry cost");
    }

    /// "First" is a claim about TIME, so the answer cannot depend on the
    /// order the events arrive in.
    #[test]
    fn repeat_is_measured_chronologically_not_by_arrival() {
        let forward =
            repeat_cost(&[load("1", "a.rs", 5), load("2", "a.rs", 500)]);
        let reverse =
            repeat_cost(&[load("2", "a.rs", 500), load("1", "a.rs", 5)]);
        assert_eq!(forward, reverse, "order of arrival must not matter");
        assert_eq!(forward.entry, 5, "the EARLIER load is the entry");
    }

    /// An empty session is zero everywhere, and no division blows up (V92).
    #[test]
    fn nothing_measured_is_zero_and_not_a_crash() {
        let got = repeat_cost(&[]);
        assert_eq!(got, Repeat::default());
        assert_eq!(pct(0, 0), None, "no denominator, no percentage");
    }

    /// The JSON reader takes the CONTRACT's shape (V9), including `null`
    /// as absent rather than as an empty string.
    #[test]
    fn the_json_reader_takes_numbers_and_null() {
        let line = r#"{"ts":"T1","path":null,"source":"shell","tokens":42}"#;
        assert_eq!(num(line, "tokens"), Some(42));
        assert_eq!(num(line, "nope"), None);
        assert_eq!(text(line, "path"), None, "null is absent, not empty");
        assert_eq!(text(line, "source"), Some("shell".to_owned()));
    }

    /// A load with no path is identified by its SOURCE in parens -- the
    /// same identity `top` groups by, so the two reports agree (V64).
    #[test]
    fn a_pathless_load_is_identified_by_its_source() {
        let line = r#"{"ts":"T1","source":"shell","path":null,"tokens":7}"#;
        assert_eq!(
            load_of(line),
            Some(load("T1", "(shell)", 7)),
            "grouped like top does"
        );
    }
}
