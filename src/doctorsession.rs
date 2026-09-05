//! `doctor --session`: doctor retargeted at a CONTEXT (V99/T76).
//!
//! A THIN COMPOSER, and the boundary is load-bearing (V17): every figure
//! here is `headroom`'s or `top`'s. This module resolves a session,
//! borrows the `df` row from one and the ranking from the other, and
//! multiplies occupancy by `~turns left` to point the same product
//! forward. It defines no estimator of its own, and it must not grow one.
//!
//! The advice block is NOT here. V99 permits exactly two suggestions and
//! forbids the intuitive third, which is a decision about what to say
//! rather than what to compute; it lands with T77.
//!
//! Report-only, exit 0 (V5). `doctor` advises, `check` gates (V17).

use crate::args::Format;
use crate::cli::Output;
use crate::session::Session;
use crate::topcmd::Caveat;
use std::path::PathBuf;

#[derive(Default)]
struct Raw {
    session: Option<String>,
    model: Option<String>,
    window: Option<String>,
    human: bool,
    format: Format,
    chdir: Option<String>,
}

/// Does this invocation target a session? Read BEFORE `args::parse`,
/// because `--session` belongs to this verb and adding it to the shared
/// `Opts` would put the flag on `estimate` and `fit` as well -- a surface
/// nothing promised and nothing implements (V106, from the other side).
pub(crate) fn targeted(rest: &[String]) -> bool {
    rest.iter().any(|a| a == "--session")
}

pub(crate) fn session(rest: &[String]) -> Output {
    match parse(rest) {
        Ok(raw) => run(&raw),
        Err(e) => Output::usage_err(format!("itok: {e}")),
    }
}

fn run(raw: &Raw) -> Output {
    let (parsed, compactions, origin) = match load(raw) {
        Ok(v) => v,
        Err(o) => return o,
    };
    let context = Context {
        parsed: &parsed,
        caveat: Caveat::of(&compactions),
        origin: &origin,
    };
    capacity(raw).map_or_else(
        |e| Output::usage_err(format!("itok: {e}")),
        |cap| report(raw, &context, cap),
    )
}

fn load(raw: &Raw) -> Result<crate::tracecmd::Loaded, Output> {
    crate::tracecmd::session_with_compactions(
        raw.session.as_deref(),
        raw.chdir.as_deref(),
    )
}

/// The shared capacity ladder (V114): `--window` first, then `--model`
/// through `.context-models`. An unknown model FAILS rather than
/// defaulting, because a wrong capacity makes `use%` and `turns left`
/// fictions that still look measured (V11/V92).
fn capacity(raw: &Raw) -> Result<Option<u64>, String> {
    let root = PathBuf::from(raw.chdir.as_deref().unwrap_or("."));
    crate::capacity::resolve(raw.window.as_deref(), raw.model.as_deref(), &root)
}

/// No usage anywhere means no row at all -- absent, never zero (V47).
fn report(raw: &Raw, context: &Context<'_>, cap: Option<u64>) -> Output {
    let Some(room) = crate::headroom::room_of(context.parsed, cap, None) else {
        return Output::ok(String::new());
    };
    let rows = projected(context.parsed, room.turns_left().map(|(n, _)| n));
    Output::ok(match raw.format {
        Format::Json => json(&room, &rows, context.caveat),
        Format::Human => {
            crate::headroom::table(&room, raw.human)
                + &human(&rows, context.caveat, raw.human)
                + &crate::tracecmd::origin_note(context.origin)
        }
    })
}

/// What one read of the transcript yielded. Grouped because `report`
/// wants all three and the argument limit is four -- and because a
/// session, its compaction record and where it came from are one answer
/// to one question, not three inputs that happen to travel together.
struct Context<'a> {
    parsed: &'a Session,
    caveat: Caveat<'a>,
    origin: &'a crate::tracecmd::Origin,
}

/// One item's billing still to come: its occupancy times the turns the
/// context has left (V100's FORWARD direction).
struct Ahead {
    what: String,
    tokens: u64,
    projected: u64,
}

/// The forward products, biggest first.
///
/// NO `turns left` means NO `projected` (V100/V92): without a capacity
/// there is no denominator, and a product against an invented one would
/// be a fiction that looks measured. An empty list is how that arrives
/// here, and the renderers say WHY rather than printing nothing.
fn projected(parsed: &Session, turns_left: Option<u64>) -> Vec<Ahead> {
    let Some(left) = turns_left else {
        return Vec::new();
    };
    crate::topcmd::ranked(parsed)
        .into_iter()
        .map(|r| Ahead {
            projected: r.tokens.saturating_mul(left),
            what: r.what,
            tokens: r.tokens,
        })
        .collect()
}

/// How many rows the block shows. `top` has `--top N`; this is a fixed
/// head because the block exists to name the few items worth acting on,
/// and a hundred-line advisory is one nobody reads (V71).
const SHOWN: usize = 5;

fn human(rows: &[Ahead], caveat: Caveat<'_>, h: bool) -> String {
    if rows.is_empty() {
        return "  projected  -- no capacity, so no `turns left` and no \
                projection (pass --window or --model)\n"
            .to_owned();
    }
    let head = format!("  projected = itok x ~turns left, {}\n", claim(caveat));
    let body: String = rows
        .iter()
        .take(SHOWN)
        .map(|a| projected_line(a, h))
        .collect();
    head + &body
}

/// The tilde is not decoration: `turns left` is an extrapolation, so a
/// product built on it is one too (V3/V93).
fn projected_line(a: &Ahead, h: bool) -> String {
    let ahead = format!("~{}", size(a.projected, h));
    format!(
        "  {:>10} itok  {ahead:>11}  {}\n",
        size(a.tokens, h),
        a.what
    )
}

/// `-h`'s rendering, the same one `headroom` prints beside this block:
/// one flag, one meaning, both tables (V64).
fn size(n: u64, h: bool) -> String {
    if h {
        crate::render::human(n)
    } else {
        n.to_string()
    }
}

/// What the product is worth on THIS session, read off its own record
/// (B29): a context that compacted has already dropped items, so both
/// directions of V98's product are upper bounds.
fn claim(caveat: Caveat<'_>) -> &'static str {
    if caveat.method().contains("UPPER BOUND") {
        "at the recent rate (UPPER BOUND: this session compacted)"
    } else {
        "at the recent rate (assumes no compaction)"
    }
}

/// ONE object (V9). `headroom`'s row is NESTED verbatim rather than
/// flattened, so the json says structurally what the module doc says in
/// prose: these figures are that verb's, unchanged.
fn json(room: &crate::headroom::Room, rows: &[Ahead], c: Caveat<'_>) -> String {
    let items: Vec<String> = rows.iter().take(SHOWN).map(json_item).collect();
    format!(
        "{{\"session\":true,\"headroom\":{},\"projected\":[{}],\
         \"projected_method\":\"{}\"}}\n",
        crate::headroom::json(room).trim_end(),
        items.join(","),
        c.method().replace("turns-since-entry", "~turns-left"),
    )
}

fn json_item(a: &Ahead) -> String {
    format!(
        "{{\"what\":\"{}\",\"tokens\":{},\"projected\":{}}}",
        a.what.replace('"', "\\\""),
        a.tokens,
        a.projected
    )
}

fn parse(rest: &[String]) -> Result<Raw, String> {
    let mut raw = Raw::default();
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        one(&mut raw, a, &mut it)?;
    }
    Ok(raw)
}

/// One argument. Split out because the loop and the vocabulary are two
/// things, and the vocabulary is the half that grows.
fn one<'a>(
    raw: &mut Raw,
    a: &str,
    it: &mut impl Iterator<Item = &'a String>,
) -> Result<(), String> {
    match a {
        "--session" => Ok(()),
        "-h" => {
            raw.human = true;
            Ok(())
        }
        "--model" | "--window" | "--format" | "-C" => with_value(raw, a, it),
        flag if flag.starts_with('-') => Err(format!("unknown flag '{flag}'")),
        positional => target(raw, positional),
    }
}

/// The ONE positional `--session` allows is the session itself (V104: an
/// id or a path). A second one is a FILESET, which this form does not
/// have -- and saying so beats silently ignoring it (V2).
fn target(raw: &mut Raw, value: &str) -> Result<(), String> {
    if raw.session.is_some() {
        return Err(format!(
            "doctor --session takes one session, not a fileset: '{value}'"
        ));
    }
    raw.session = Some(value.to_owned());
    Ok(())
}

fn with_value<'a>(
    raw: &mut Raw,
    flag: &str,
    it: &mut impl Iterator<Item = &'a String>,
) -> Result<(), String> {
    let value = it
        .next()
        .ok_or_else(|| format!("{flag} needs a value"))?
        .clone();
    match flag {
        "--model" => raw.model = Some(value),
        "--window" => raw.window = Some(value),
        "-C" => raw.chdir = Some(value),
        _ => raw.format = crate::tracecmd::format_of(&value)?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{LoadEvent, Source, Turn};

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| (*s).to_owned()).collect()
    }

    /// A session with two loads and a growing window, so `turns left`
    /// has a rate to extrapolate from.
    fn session_of() -> Session {
        Session {
            turns: (1..=4u64)
                .map(|i| Turn {
                    input: Some(i.saturating_mul(10_000)),
                    ts: format!("2026-09-0{i}T06:00:00.000Z"),
                    ..Turn::default()
                })
                .collect(),
            events: vec![load("big.rs", 8_000), load("small.rs", 400)],
            ..Session::default()
        }
    }

    fn load(path: &str, bytes: usize) -> LoadEvent {
        LoadEvent {
            session: "s1".to_owned(),
            ts: "2026-09-01T06:00:00.000Z".to_owned(),
            source: Source::Tool("Read".to_owned()),
            path: Some(path.to_owned()),
            bytes,
            spilled: None,
        }
    }

    fn ahead(what: &str, tokens: u64, projected: u64) -> Ahead {
        Ahead {
            what: what.to_owned(),
            tokens,
            projected,
        }
    }

    #[test]
    fn the_flag_is_what_routes_the_verb() {
        assert!(targeted(&args(&["--session"])));
        assert!(targeted(&args(&["--window", "1M", "--session", "abc"])));
        assert!(!targeted(&args(&["SPEC.md"])));
    }

    /// V2: `--session` has no fileset. A second positional is a path
    /// somebody expected to be read, and silence would let them believe
    /// it was.
    #[test]
    fn a_second_positional_is_a_usage_error_naming_the_rule() {
        let err = parse(&args(&["--session", "one", "two"]));
        let msg = err.err().unwrap_or_default();
        assert!(msg.contains("not a fileset"), "{msg}");
        assert!(msg.contains("'two'"), "names the offender: {msg}");
    }

    #[test]
    fn the_one_positional_is_the_session() {
        let raw = parse(&args(&["--session", "abc"])).unwrap_or_default();
        assert_eq!(raw.session.as_deref(), Some("abc"));
    }

    #[test]
    fn an_unknown_flag_is_a_usage_error() {
        assert!(parse(&args(&["--session", "--nope"])).is_err());
        assert!(parse(&args(&["--session", "--window"])).is_err());
    }

    /// V100/V92: no capacity means no `turns left`, and a product built
    /// on an invented denominator is a fiction that looks measured.
    #[test]
    fn without_turns_left_there_is_no_projection_at_all() {
        assert!(projected(&session_of(), None).is_empty());
    }

    /// The forward direction: occupancy times the turns remaining,
    /// biggest first, from `top`'s ranking rather than a new estimator.
    #[test]
    fn projection_is_occupancy_times_the_turns_remaining() {
        let rows = projected(&session_of(), Some(10));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows.first().map(|r| r.projected), Some(20_000));
        assert_eq!(rows.get(1).map(|r| r.projected), Some(1_000));
    }

    /// The absence is EXPLAINED, not printed as an empty block: a reader
    /// who sees nothing cannot tell "no projection" from "nothing to
    /// project" (V47's rule, on a report).
    #[test]
    fn an_empty_projection_says_why() {
        let out = human(&[], Caveat::of(&[]), false);
        assert!(out.contains("no capacity"), "{out}");
        assert!(out.contains("--window"), "names the fix: {out}");
    }

    /// B29 reaches the forward direction too: a session that compacted
    /// has already dropped items, so this product is an upper bound.
    #[test]
    fn a_compacted_session_labels_the_projection_an_upper_bound() {
        let rows = vec![ahead("big.rs", 8_000, 80_000)];
        let plain = human(&rows, Caveat::of(&[]), false);
        assert!(plain.contains("assumes no compaction"), "{plain}");
        assert!(!plain.contains("UPPER BOUND"), "{plain}");
    }

    /// V3/V93: `turns left` is an extrapolation, so a product built on
    /// it wears the tilde too.
    #[test]
    fn every_projected_cell_carries_the_tilde() {
        let rows = vec![ahead("big.rs", 8_000, 80_000)];
        let out = human(&rows, Caveat::of(&[]), false);
        assert!(out.contains("~80000"), "{out}");
        assert!(
            out.contains("at the recent rate"),
            "assumption named: {out}"
        );
    }

    /// V99's boundary, asserted BEFORE the advice block exists (T77):
    /// this report computes, it does not judge. The forbidden suggestion
    /// is the intuitive one, which is why omission alone is not enough.
    #[test]
    fn the_session_report_gives_no_advice() {
        let rows = vec![ahead("big.rs", 8_000, 80_000)];
        let out = human(&rows, Caveat::of(&[]), false);
        for word in ["evict", "should", "consider", "reduce", "unhealthy"] {
            assert!(!out.to_lowercase().contains(word), "no advice: {word}");
        }
    }

    /// V9: one object, and `headroom`'s row nested verbatim -- the json
    /// says structurally what the module says in prose.
    #[test]
    fn json_is_one_object_nesting_headrooms_own() {
        let session = session_of();
        let room = crate::headroom::room_of(&session, Some(1_000_000), None);
        let room = room.unwrap_or_else(|| unreachable!("turns carry usage"));
        let rows = vec![ahead("big.rs", 8_000, 80_000)];
        let out = json(&room, &rows, Caveat::of(&[]));
        assert_eq!(out.lines().count(), 1, "{out}");
        assert!(out.contains("\"headroom\":{\"window\":1000000"), "{out}");
        assert!(out.contains("\"projected\":[{\"what\":\"big.rs\""), "{out}");
        assert!(out.contains("\"projected_method\":"), "{out}");
    }
}
