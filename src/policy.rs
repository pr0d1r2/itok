//! `.context-policy`: the runtime registry (V57).
//!
//! The third config file, deliberately NOT a third config language: the
//! same shape both siblings already use -- whitespace-separated fields,
//! `#` comments, blank lines skipped -- and the same decimal unit grammar
//! as `--budget` and `--window` (V18). One thing to learn, not three.
//!
//! OPT-IN, like `.context-limits` (V10). An absent file is an empty
//! policy, never an error and never a default: with no policy and no
//! installed hook, itok is exactly as report-only as it is without this
//! module (V53). Enforcement never self-enables.
//!
//! An unparsable row FAILS (V88). A row the tool cannot read is a row the
//! AUTHOR believes is enforced, so skipping it silently turns a gate into
//! decoration -- strictly worse than having no row. The diagnostic names
//! the file, the line, and what was expected, because "bad row" leaves the
//! author to find it.
//!
//! PARSER ONLY. It reads and validates; it decides nothing. Matching a
//! pattern against a path belongs to the guard (T43) and the tier
//! machinery to the fuse (T44), so a pattern is carried here as an opaque
//! string. That is also why no glob matcher appears below: V57 says this
//! file reuses "the glob semantics of the existing registries", and those
//! registries match exact names today. Inventing semantics here would hand
//! T43 an answer nobody reviewed.

use crate::units;

pub(crate) const POLICY: &str = ".context-policy";

/// One budget: an opaque pattern and its ceiling in tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Budget {
    /// A path pattern or a tool name. Opaque here -- see the module note.
    pub(crate) what: String,
    pub(crate) tokens: u64,
}

/// A fuse tier, in V54's order: ledger-only, then louder, then lossy, then
/// refusing. The set is CLOSED -- a fifth name is a row this tool cannot
/// honor, and honoring it silently would be the decoration V88 forbids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Level {
    Observe,
    Warn,
    Cap,
    Deny,
}

impl Level {
    fn of(word: &str) -> Option<Self> {
        match word {
            "observe" => Some(Self::Observe),
            "warn" => Some(Self::Warn),
            "cap" => Some(Self::Cap),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Observe => "observe",
            Self::Warn => "warn",
            Self::Cap => "cap",
            Self::Deny => "deny",
        }
    }
}

/// One occupancy tier: at `pct` percent of the window, this level applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Tier {
    pub(crate) level: Level,
    pub(crate) pct: u64,
}

/// The sliding-window rate fuse (V54): `tokens` within `calls` calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rate {
    pub(crate) tokens: u64,
    pub(crate) calls: u64,
}

/// The parsed registry. Empty is the honest default (V53).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Policy {
    pub(crate) budgets: Vec<Budget>,
    pub(crate) tools: Vec<Budget>,
    /// Paths that are never capped, elided, or denied (V56). Order is the
    /// file's, so a reader can find a pin where they wrote it.
    pub(crate) pins: Vec<String>,
    pub(crate) tiers: Vec<Tier>,
    pub(crate) rate: Option<Rate>,
}

impl Policy {
    /// Whether anything at all was registered. `false` is what V53 means by
    /// "no policy": the caller enforces nothing rather than enforcing an
    /// empty set, and the two must stay distinguishable.
    pub(crate) fn is_empty(&self) -> bool {
        self.budgets.is_empty()
            && self.tools.is_empty()
            && self.pins.is_empty()
            && self.tiers.is_empty()
            && self.rate.is_none()
    }
}

/// Read `<root>/.context-policy`. ABSENT IS EMPTY, never an error (V10/V53).
///
/// Unreadable-for-another-reason is also empty, matching `.context-limits`:
/// this registry is opt-in, so "no policy" is the resting state and the
/// caller enforces nothing. That is the opposite of V88, which governs a
/// file that EXISTS and carries a row this tool cannot read.
pub(crate) fn read(root: &std::path::Path) -> Result<Policy, String> {
    match std::fs::read_to_string(root.join(POLICY)) {
        Ok(t) => parse(&t),
        Err(_) => Ok(Policy::default()),
    }
}

/// Every row, FAILING on the first unreadable one (V88).
pub(crate) fn parse(text: &str) -> Result<Policy, String> {
    let mut out = Policy::default();
    for (n, line) in rows(text) {
        let row = row_of(line).ok_or_else(|| row_err(n, line))?;
        absorb(&mut out, row, n)?;
    }
    Ok(out)
}

/// The rows that carry content, with their 1-based line numbers.
fn rows(text: &str) -> Vec<(usize, &str)> {
    text.lines()
        .enumerate()
        .map(|(i, l)| (i.saturating_add(1), l.trim()))
        .filter(|(_, l)| !l.is_empty() && !l.starts_with('#'))
        .collect()
}

/// One row's meaning, before it is folded into the policy.
enum Row {
    Budget(Budget),
    Tool(Budget),
    Pin(String),
    Tier(Tier),
    Rate(Rate),
}

/// Dispatch on the FIRST field, which names the row's kind.
///
/// A kind keyword rather than inferred shape: `pin SPEC.md` and
/// `budget SPEC.md 20k` differ only in arity, and a parser that guessed
/// from field count would read a typo'd budget as a pin -- silently
/// granting absolute protection to something the author meant to cap.
fn row_of(line: &str) -> Option<Row> {
    let mut f = line.split_whitespace();
    let kind = f.next()?;
    let a = f.next()?;
    let b = f.next();
    if f.next().is_some() {
        return None;
    }
    build(kind, a, b)
}

fn build(kind: &str, a: &str, b: Option<&str>) -> Option<Row> {
    match kind {
        "budget" => Some(Row::Budget(budget(a, b?)?)),
        "tool" => Some(Row::Tool(budget(a, b?)?)),
        "pin" => b.is_none().then(|| Row::Pin(a.to_owned())),
        "fuse" => Some(Row::Tier(tier(a, b?)?)),
        "rate" => Some(Row::Rate(rate(a, b?)?)),
        _ => None,
    }
}

/// A pattern and its ceiling, through the ONE unit grammar (V18).
fn budget(what: &str, count: &str) -> Option<Budget> {
    Some(Budget {
        what: what.to_owned(),
        tokens: units::parse(count).ok()?,
    })
}

/// A tier: a closed level name, and a percentage that is really one.
/// `fuse warn 150` is not a percentage, so it is an unreadable row (V88)
/// rather than a threshold that can never trip.
fn tier(level: &str, pct: &str) -> Option<Tier> {
    let pct = pct.parse::<u64>().ok().filter(|p| *p <= 100)?;
    Some(Tier {
        level: Level::of(level)?,
        pct,
    })
}

/// The rate fuse, both fields through the unit grammar. Zero calls is not
/// a window, so it cannot be a rate.
fn rate(tokens: &str, calls: &str) -> Option<Rate> {
    let calls = units::parse(calls).ok().filter(|c| *c > 0)?;
    Some(Rate {
        tokens: units::parse(tokens).ok()?,
        calls,
    })
}

/// Fold one row in, refusing a second declaration of the same thing.
///
/// A repeated tier or a second rate is a CONTRADICTION, not a preference:
/// the author wrote both expecting both, and last-wins would enforce one
/// while reading as if it enforced the other. Same reasoning as V88, one
/// level up from syntax.
fn absorb(out: &mut Policy, row: Row, n: usize) -> Result<(), String> {
    match row {
        Row::Budget(b) => out.budgets.push(b),
        Row::Tool(t) => out.tools.push(t),
        Row::Pin(p) => out.pins.push(p),
        Row::Tier(t) => return add_tier(out, t, n),
        Row::Rate(r) => return add_rate(out, r, n),
    }
    Ok(())
}

fn add_tier(out: &mut Policy, t: Tier, n: usize) -> Result<(), String> {
    if out.tiers.iter().any(|o| o.level == t.level) {
        return Err(format!(
            "{POLICY}:{n}: tier `{}` is declared twice; \
             a tier has one threshold",
            t.level.label()
        ));
    }
    out.tiers.push(t);
    Ok(())
}

fn add_rate(out: &mut Policy, r: Rate, n: usize) -> Result<(), String> {
    if out.rate.is_some() {
        return Err(format!(
            "{POLICY}:{n}: `rate` is declared twice; there is one rate fuse"
        ));
    }
    out.rate = Some(r);
    Ok(())
}

/// Rows this BUILD cannot honor yet, named with the task that will.
///
/// The split is `models.rs`'s, and it is the honest one: V88 governs a row
/// that is MALFORMED, while this governs a row that is well-formed and not
/// yet implemented. Collapsing them would either reject a legal file or --
/// far worse -- accept a `fuse` tier and enforce nothing, which is the
/// false assurance V69/V105 exist to prevent. A user who writes a tier
/// today gets told so, rather than believing it is live.
pub(crate) fn unhonored(p: &Policy) -> Vec<String> {
    let mut out = Vec::new();
    if !p.tiers.is_empty() {
        out.push(format!(
            "{POLICY}: `fuse` tiers are parsed but not honored yet (T44)"
        ));
    }
    if p.rate.is_some() {
        out.push(format!(
            "{POLICY}: the `rate` fuse is parsed but not honored yet (T44)"
        ));
    }
    out
}

/// Names the FILE, the LINE, and what was expected (V88).
fn row_err(line: usize, text: &str) -> String {
    format!(
        "{POLICY}:{line}: expected one of \
         `budget <pattern> <count>` | `tool <name> <count>` | \
         `pin <path>` | `fuse observe|warn|cap|deny <percent>` | \
         `rate <count> <calls>` (count like 20000, 20k, 1M), got `{text}`"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// V53/V10: no file is an EMPTY policy, not an error and not a default.
    /// With nothing registered the caller enforces nothing, which is what
    /// "enforcement never self-enables" has to mean in code.
    #[test]
    fn an_absent_file_is_an_empty_policy() {
        let dir = std::env::temp_dir().join("itok-policy-absent");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::remove_file(dir.join(POLICY));
        assert_eq!(read(&dir), Ok(Policy::default()));
        assert!(Policy::default().is_empty(), "and it reads as empty");
    }

    /// A file of nothing but comments is also empty -- and still parses,
    /// because an author commenting a policy out has disabled it, not
    /// broken it.
    #[test]
    fn comments_and_blanks_are_not_rows() {
        let got = parse("# a note\n\n   \n# budget src 1k\n");
        assert_eq!(got, Ok(Policy::default()));
    }

    /// V88, PLANTED: every malformed shape FAILS, and the diagnostic names
    /// the file, the line and what was expected. A guard proven by reading
    /// is indistinguishable from one that cannot reject (V79).
    #[test]
    fn an_unreadable_row_fails_and_says_where() {
        for (text, why) in [
            ("budget src/**/*.rs\n", "a budget with no count"),
            ("budget src 20.5k\n", "a count the grammar cannot read"),
            ("pin SPEC.md 20k\n", "a pin with a count"),
            ("fuse warn\n", "a tier with no percent"),
            ("rate 200k\n", "a rate with no call window"),
            ("budget a 1k extra\n", "a trailing field"),
        ] {
            let Err(e) = parse(text) else {
                unreachable!("{why} must not parse: {text:?}");
            };
            assert!(e.starts_with(&format!("{POLICY}:1:")), "{e}");
            assert!(e.contains("expected"), "{e}");
        }
    }

    /// V88 again: an unknown KIND is a row this tool cannot honor, so it
    /// fails rather than being skipped. Skipping would leave the author
    /// believing a rule is enforced.
    #[test]
    fn an_unknown_row_kind_fails_rather_than_being_skipped() {
        let Err(e) = parse("budget a 1k\nallow b\n") else {
            unreachable!("an unknown kind must not be skipped");
        };
        assert!(e.starts_with(&format!("{POLICY}:2:")), "{e}");
    }

    /// V18: the one decimal unit grammar, shared with `--budget`/`--window`.
    #[test]
    fn counts_use_the_one_unit_grammar() {
        let got = parse("budget a 20000\nbudget b 20k\nbudget c 1M\n");
        let want = ["a", "b", "c"].iter().zip([20_000, 20_000, 1_000_000]);
        let Ok(p) = got else {
            unreachable!("all three forms parse");
        };
        let seen: Vec<(&str, u64)> = p
            .budgets
            .iter()
            .map(|b| (b.what.as_str(), b.tokens))
            .collect();
        let expect: Vec<(&str, u64)> = want.map(|(w, n)| (*w, n)).collect();
        assert_eq!(seen, expect);
    }

    /// V56: pins carry no count and keep the file's order, so a reader
    /// finds a pin where they wrote it.
    #[test]
    fn pins_have_no_count_and_keep_their_order() {
        let Ok(p) = parse("pin SPEC.md\npin CLAUDE.md\n") else {
            unreachable!("two pins parse");
        };
        assert_eq!(p.pins, vec!["SPEC.md", "CLAUDE.md"]);
        assert!(!p.is_empty(), "pins alone are a policy");
    }

    /// V54: the tier set is CLOSED. A fifth name is not a tier this tool
    /// can honor, and a percentage over 100 can never trip.
    #[test]
    fn the_tier_set_is_closed_and_a_percent_is_a_percent() {
        let all = "fuse observe 50\nfuse warn 70\nfuse cap 85\nfuse deny 95\n";
        let Ok(p) = parse(all) else {
            unreachable!("all four tiers parse");
        };
        assert_eq!(p.tiers.len(), 4);
        assert!(parse("fuse panic 70\n").is_err(), "a fifth name");
        assert!(parse("fuse warn 150\n").is_err(), "not a percentage");
    }

    /// A repeated tier is a CONTRADICTION, not a preference: last-wins
    /// would enforce one threshold while the file states two.
    #[test]
    fn a_tier_declared_twice_fails() {
        let Err(e) = parse("fuse warn 70\nfuse warn 80\n") else {
            unreachable!("two thresholds for one tier must fail");
        };
        assert!(e.contains("declared twice"), "{e}");
        assert!(parse("rate 1k 5\nrate 2k 5\n").is_err(), "and one rate");
    }

    /// A zero-call window is not a window, so it is not a rate.
    #[test]
    fn a_rate_needs_a_real_call_window() {
        assert!(parse("rate 200k 0\n").is_err(), "zero calls");
        assert_eq!(
            parse("rate 200k 10\n").map(|p| p.rate),
            Ok(Some(Rate {
                tokens: 200_000,
                calls: 10
            }))
        );
    }

    /// The whole shape at once, as an author would write it.
    #[test]
    fn a_realistic_policy_parses_to_what_it_says() {
        let text = "# runtime policy\n\
                    budget src/**/*.rs 20k\n\
                    tool Bash 10k\n\
                    pin SPEC.md\n\
                    fuse warn 70\n\
                    fuse deny 95\n\
                    rate 200k 10\n";
        let Ok(p) = parse(text) else {
            unreachable!("the documented shape parses");
        };
        assert_eq!(p.budgets.len(), 1);
        assert_eq!(p.tools.len(), 1);
        assert_eq!(p.pins, vec!["SPEC.md"]);
        assert_eq!(p.tiers.len(), 2, "a partial tier set is legal");
        assert_eq!(p.rate.map(|r| r.calls), Some(10));
    }

    /// The pattern is OPAQUE here (see the module note): the parser stores
    /// what the author wrote and matches nothing, so T43 inherits no
    /// semantics this task invented.
    #[test]
    fn a_pattern_is_stored_verbatim() {
        let Ok(p) = parse("budget src/**/*.rs 1k\n") else {
            unreachable!("a glob-shaped pattern parses");
        };
        assert_eq!(
            p.budgets.first().map(|b| b.what.as_str()),
            Some("src/**/*.rs")
        );
    }

    /// V105: a row this build cannot honor is NAMED, never silently
    /// accepted. Accepting a `fuse` tier and enforcing nothing is the
    /// false assurance the whole B15 record is about.
    #[test]
    fn a_row_this_build_cannot_honor_yet_is_named() {
        let Ok(p) = parse("fuse warn 70\nrate 1k 5\n") else {
            unreachable!("both rows are well formed");
        };
        let said = unhonored(&p);
        assert_eq!(said.len(), 2, "one per unhonored kind");
        assert!(said.iter().any(|m| m.contains("T44")), "{said:?}");
    }

    /// What T43 DOES honor says nothing, so a policy of budgets, tools and
    /// pins runs silently -- success is silence (V71).
    #[test]
    fn the_honored_rows_are_silent() {
        let Ok(p) = parse("budget a 1k\ntool Bash 2k\npin SPEC.md\n") else {
            unreachable!("three honored rows");
        };
        assert!(unhonored(&p).is_empty(), "nothing to say");
    }

    /// Every level renders as the word it was parsed from -- one spelling,
    /// so a diagnostic and the file agree.
    #[test]
    fn a_level_round_trips_through_its_label() {
        for word in ["observe", "warn", "cap", "deny"] {
            assert_eq!(Level::of(word).map(Level::label), Some(word));
        }
        assert_eq!(Level::of("nope"), None);
    }
}
