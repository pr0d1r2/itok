//! The compaction machine (T70): CPU derives the candidates, CPU verifies
//! the result, and exactly one inference call does the rewriting in
//! between.
//!
//! Compaction is a COST control, not tidiness (V84): `SPEC.md` is re-sent
//! on every turn of every session, so every byte is a tax on all future
//! work. Measured on one real session, the spec alone carried ~7M itok.
//!
//! What makes cutting SAFE is V26 -- method lives in the commit trail, not
//! the shipped text. Every invariant here landed with a long commit
//! message, so the argumentation already survives outside this file and
//! the spec can keep the RULE, its measured numbers, and its citations.
//!
//! What makes cutting HONEST is that a model cannot audit its own rewrite
//! (V80): the context that produced an edit shares its blind spot. So the
//! must-keep set is derived mechanically and checked mechanically, and a
//! deliberate removal has to be NAMED (V73) rather than merely intended.
//!
//! Lives as a test, not a second binary: `cargo nextest run` executes the
//! units below on every commit, so the verifier cannot rot, while the
//! by-hand report is `#[ignore]` -- the shape this repo already uses for
//! machinery that must compile but not run in the gate (V38).
//!
//!   cargo test --test spec_compact -- --ignored --nocapture
//!   ITOK_ALLOW_DROP=tok,tok cargo test --test spec_compact -- --ignored

use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

const DIR: &str = env!("CARGO_MANIFEST_DIR");

// ---------------------------------------------------------------- derive

/// One invariant's size and how many times the rest of the spec cites it.
#[derive(Debug, PartialEq, Eq)]
struct Inv {
    bytes: usize,
    cited: usize,
    /// A WORKING principle (V80-V87) rather than a product rule.
    working: bool,
}

/// The working-principle band: these bind how the work is done, not what
/// is built.
///
/// Held apart because low citation means opposite things on either side of
/// the line. A product invariant nobody cites is a candidate; a process
/// invariant nobody cites is NORMAL -- tasks cite the rules they
/// implement, and no task "implements" refute-before-you-trust. A naive
/// orphan detector would recommend deleting exactly the invariants that
/// govern the work (V80/V83/V85/V86 are all uncited today).
const WORKING: std::ops::RangeInclusive<u32> = 80..=87;

/// Byte size of each invariant, keyed by id, plus its citation in-degree.
fn invariants(spec: &str) -> BTreeMap<String, Inv> {
    let body = section(spec, "INVARIANTS");
    let cites = citation_counts(spec);
    let mut out = BTreeMap::new();
    let mut cur: Option<(String, usize)> = None;
    for line in body.lines() {
        let n = line.len().saturating_add(1);
        if let Some(id) = declared_id(line) {
            flush(&mut out, cur.take(), &cites);
            cur = Some((id, n));
        } else {
            grow(cur.as_mut(), n);
        }
    }
    flush(&mut out, cur, &cites);
    out
}

/// Add a continuation line's bytes to the invariant being accumulated.
fn grow(current: Option<&mut (String, usize)>, n: usize) {
    if let Some((_, bytes)) = current {
        *bytes = bytes.saturating_add(n);
    }
}

fn flush(
    out: &mut BTreeMap<String, Inv>,
    cur: Option<(String, usize)>,
    cites: &BTreeMap<String, usize>,
) {
    if let Some((id, bytes)) = cur {
        let num = id.trim_start_matches('V').parse::<u32>().unwrap_or(0);
        out.insert(
            id.clone(),
            Inv {
                bytes,
                // Minus one: a declaration is not a citation of itself.
                cited: cites.get(&id).copied().unwrap_or(0).saturating_sub(1),
                working: WORKING.contains(&num),
            },
        );
    }
}

/// `V42:` at the start of a line -> `V42`.
fn declared_id(line: &str) -> Option<String> {
    let rest = line.strip_prefix('V')?;
    let idx = rest.find(':')?;
    let num = rest.get(..idx)?;
    let ok = !num.is_empty() && num.chars().all(|c| c.is_ascii_digit());
    ok.then(|| format!("V{num}"))
}

/// How often each `Vnn` appears anywhere in the spec, declarations
/// included -- the caller subtracts its own.
fn citation_counts(spec: &str) -> BTreeMap<String, usize> {
    let mut out: BTreeMap<String, usize> = BTreeMap::new();
    for tok in ids_in(spec, 'V') {
        let slot = out.entry(tok).or_insert(0usize);
        *slot = slot.saturating_add(1);
    }
    out
}

/// Every `<prefix><digits>` token, e.g. every `V42` / `T30` / `B7`.
fn ids_in(text: &str, prefix: char) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter_map(|t| {
            let num = t.strip_prefix(prefix)?;
            let ok = !num.is_empty() && num.chars().all(|c| c.is_ascii_digit());
            ok.then(|| t.to_owned())
        })
        .collect()
}

/// One `## section X NAME` section's body, by its NAME.
fn section(spec: &str, name: &str) -> String {
    let start = spec.find(name).map_or(0, |i| i);
    let rest = spec.get(start..).unwrap_or_default();
    let end = rest.find("\n## ").unwrap_or(rest.len());
    rest.get(..end).unwrap_or_default().to_owned()
}

// ---------------------------------------------------------------- verify

/// Everything a rewrite must not silently lose.
///
/// Three classes, each chosen because losing one is invisible to a byte
/// count and to a human skim:
///
/// - IDs (`V42`, `T30`, `B7`): a dropped citation breaks the audit trail,
///   and `spec_integrity` only catches a citation pointing at NOTHING --
///   not one that quietly stopped pointing at all.
/// - NUMBERS: the measured evidence. `~250k`, `98`, `32,074` are the
///   difference between an invariant and an opinion (V3).
/// - BACKTICKED identifiers: flag names, file names, verb names. A rewrite
///   that renames `--allow-drop` to `--allowDrop` in prose has published a
///   flag that does not exist.
fn must_keep(spec: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for p in ['V', 'T', 'B'] {
        out.extend(ids_in(spec, p));
    }
    out.extend(numbers(spec));
    out.extend(backticked(spec));
    out
}

/// Digit runs, with separators stripped so `24,927` and `24927` are the
/// same fact. Single digits are skipped: they are prose ("2 hosts", "one
/// of 3"), and treating them as facts would make the verifier fire on any
/// rewording.
fn numbers(spec: &str) -> BTreeSet<String> {
    let cleaned: String = without_ids(spec)
        .chars()
        .filter(|c| *c != ',' && *c != '_')
        .collect();
    cleaned
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| s.len() >= 2)
        .map(str::to_owned)
        .collect()
}

/// The text with every `V42`/`T30`/`B7` token blanked out.
///
/// An id and its digits are ONE fact, counted once as the id. Without this
/// an allowed `V42` drop still trips on a bare `42`, so `--allow-drop`
/// would not actually allow anything -- and a hatch that cannot be used
/// gets bypassed wholesale (V71).
fn without_ids(spec: &str) -> String {
    let mut out = String::with_capacity(spec.len());
    let mut chars = spec.chars().peekable();
    while let Some(c) = chars.next() {
        if !matches!(c, 'V' | 'T' | 'B') {
            out.push(c);
            continue;
        }
        // An id was here, so its digits are not a number of their own.
        let digits = eat_digits(&mut chars);
        out.push(if digits == 0 { c } else { ' ' });
    }
    out
}

fn eat_digits(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> usize {
    let mut n = 0usize;
    while chars.peek().is_some_and(char::is_ascii_digit) {
        let _ = chars.next();
        n = n.saturating_add(1);
    }
    n
}

/// Text inside single backticks -- the identifiers the spec promises.
fn backticked(spec: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut parts = spec.split('`');
    // Skip the text before the first backtick, then take every other run.
    let _ = parts.next();
    let mut inside = true;
    for p in parts {
        if inside && !p.is_empty() && !p.contains('\n') {
            out.insert(p.to_owned());
        }
        inside = !inside;
    }
    out
}

/// Tokens present in `old` but missing from `new`, minus the ones a caller
/// deliberately dropped.
///
/// `allow` is the V73 half: an exemption NAMES what is deliberate. Without
/// it the only way past the verifier is to put the fact back, which is the
/// point -- but a removal that IS intended must be sayable, or the tool
/// gets bypassed wholesale and stops protecting anything (V71's no-bypass
/// reasoning, applied to itself).
fn lost(old: &str, new: &str, allow: &BTreeSet<String>) -> BTreeSet<String> {
    // SET DIFFERENCE, not substring containment. The first version asked
    // `new.contains(token)`, which reported every comma-separated number as
    // lost: `must_keep` normalizes `1,000,000` to `1000000`, and that
    // string appears nowhere in the raw text. Comparing like against like
    // is the only version that can be right -- and the bug surfaced on the
    // first real pass, which is the argument for the verifier existing.
    let kept = must_keep(new);
    must_keep(old)
        .into_iter()
        .filter(|t| !allow.contains(t))
        .filter(|t| !kept.contains(t))
        .collect()
}

fn allow_from_env() -> BTreeSet<String> {
    parse_allow(&std::env::var("ITOK_ALLOW_DROP").unwrap_or_default())
}

/// A comma list of deliberately-dropped tokens. Pure, so the parse is
/// testable without mutating the environment -- which would be a race the
/// moment the suite runs parallel (V68/B5).
fn parse_allow(raw: &str) -> BTreeSet<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn spec_now() -> String {
    let p = std::path::Path::new(DIR).join("SPEC.md");
    std::fs::read_to_string(p).unwrap_or_default()
}

/// `SPEC.md` as of a git ref -- the baseline a rewrite is checked against.
fn spec_at(git_ref: &str) -> String {
    let path = format!("{git_ref}:./SPEC.md");
    Command::new("git")
        .arg("-C")
        .arg(DIR)
        .args(["show", &path])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

// ----------------------------------------------------------- by-hand use

/// The derivation report: what to cut, biggest first.
///
/// `#[ignore]` because it is a tool, not an assertion -- the gate compiles
/// it so it cannot rot, and a human runs it when paying down the debt
/// (V38's shape, from `live_ollama_smoke`).
#[test]
#[ignore = "derivation report; run with --ignored --nocapture"]
fn derive_report() {
    let spec = spec_now();
    let inv = invariants(&spec);
    let total: usize = inv.values().map(|i| i.bytes).sum();
    println!("\n{} invariants, {total} bytes in section V\n", inv.len());
    let mut by_size: Vec<_> = inv.iter().collect();
    by_size.sort_by_key(|(_, i)| std::cmp::Reverse(i.bytes));
    print_biggest(&by_size);
    let orphans: Vec<_> = by_size
        .iter()
        .filter(|(_, i)| i.cited == 0 && !i.working)
        .map(|(id, _)| id.as_str())
        .collect();
    println!("\nuncited PRODUCT invariants (candidates): {orphans:?}");
    println!("must-keep tokens: {}", must_keep(&spec).len());
}

fn print_biggest(by_size: &[(&String, &Inv)]) {
    println!("{:6}{:>7}{:>7}  kind", "id", "bytes", "cited");
    for (id, i) in by_size.iter().take(20) {
        let kind = if i.working { "working" } else { "product" };
        println!("{id:6}{:7}{:7}  {kind}", i.bytes, i.cited);
    }
}

/// The verification pass: run AFTER a rewrite, against the pre-rewrite
/// spec.
///
/// `#[ignore]` for the same reason the derivation is, plus one of its own:
/// as a gate step it would forbid every legitimate wording change that
/// drops a number, which is the noise-nobody-reads failure (V71). It is a
/// deliberate check at a deliberate moment.
#[test]
#[ignore = "verification pass; run with --ignored --nocapture"]
fn verify_against_head() {
    let base = std::env::var("ITOK_BASE").unwrap_or_else(|_| "HEAD".to_owned());
    let old = spec_at(&base);
    assert!(!old.is_empty(), "no SPEC.md at {base}");
    let missing = lost(&old, &spec_now(), &allow_from_env());
    for t in &missing {
        println!("LOST: {t}");
    }
    assert!(
        missing.is_empty(),
        "{} fact(s) vanished vs {base}; restore them or name them in \
         ITOK_ALLOW_DROP",
        missing.len()
    );
}

// ---------------------------------------------------------------- units

#[cfg(test)]
mod tests {
    use super::*;

    /// The derivation finds each invariant and sizes it, including the
    /// continuation lines -- an invariant is a paragraph, not a line.
    #[test]
    fn invariants_are_sized_across_their_continuation_lines() {
        let spec =
            "## \u{a7}V INVARIANTS\nV1: **a.** one\ntwo\nV2: **b.** three\n\
                    \n## \u{a7}T TASKS\n";
        let inv = invariants(spec);
        assert_eq!(inv.len(), 2);
        let v1 = inv.get("V1").map(|i| i.bytes).unwrap_or(0);
        let v2 = inv.get("V2").map(|i| i.bytes).unwrap_or(0);
        assert!(v1 > v2, "V1 spans two lines, V2 one: {v1} vs {v2}");
    }

    /// Citation counts exclude an invariant's own declaration -- otherwise
    /// every invariant looks cited once and no orphan is ever found.
    #[test]
    fn a_declaration_is_not_a_citation_of_itself() {
        let spec = "## \u{a7}V INVARIANTS\nV1: **a.** nothing points here\n\
                    V2: **b.** but this cites V1\n\n## \u{a7}T TASKS\n";
        let inv = invariants(spec);
        assert_eq!(inv.get("V1").map(|i| i.cited), Some(1));
        assert_eq!(inv.get("V2").map(|i| i.cited), Some(0), "V2 is uncited");
    }

    /// V80-V87 are WORKING principles: uncited is normal there, and the
    /// report must not offer them as candidates.
    #[test]
    fn working_principles_are_held_apart_from_product_rules() {
        let spec = "## \u{a7}V INVARIANTS\nV79: **p.** product\n\
                    V83: **w.** working\n\n## \u{a7}T TASKS\n";
        let inv = invariants(spec);
        assert_eq!(inv.get("V79").map(|i| i.working), Some(false));
        assert_eq!(inv.get("V83").map(|i| i.working), Some(true));
    }

    /// V80's rule mechanised: a dropped fact is CAUGHT, because a rewrite
    /// cannot be trusted to audit itself.
    #[test]
    fn a_dropped_id_number_or_identifier_is_caught() {
        const OLD: &str = "V1: cites V42, measured 24,927, flag `--allow-drop`";
        let gone = |new: &str| lost(OLD, new, &BTreeSet::new());
        assert!(
            gone("V1: cites V42, measured 24927, `--allow-drop`").is_empty(),
            "a separator change is the SAME fact"
        );
        assert!(gone("V1: 24,927, `--allow-drop`").contains("V42"));
        assert!(gone("V1: cites V42, `--allow-drop`").contains("24927"));
        assert!(gone("V1: cites V42, 24,927").contains("--allow-drop"));
    }

    /// A renamed identifier reads as a loss, which is the point: prose
    /// promising `--allowDrop` promises a flag that does not exist.
    #[test]
    fn a_renamed_identifier_is_a_loss() {
        let none = BTreeSet::new();
        let got = lost("`--allow-drop`", "`--allowDrop`", &none);
        assert!(got.contains("--allow-drop"), "{got:?}");
    }

    /// V73: a deliberate removal is NAMED, and only the named one passes.
    #[test]
    fn an_allowed_drop_passes_and_only_that_one() {
        let mut allow = BTreeSet::new();
        allow.insert("V42".to_owned());
        let old = "cites V42 and V43";
        assert!(lost(old, "cites V43", &allow).is_empty());
        assert!(
            lost(old, "cites V42", &allow).contains("V43"),
            "an unnamed drop still fails"
        );
    }

    /// Single digits are prose, not facts: counting them would make the
    /// verifier fire on any rewording and get it bypassed wholesale.
    #[test]
    fn single_digits_are_not_treated_as_facts() {
        let none = BTreeSet::new();
        assert!(lost("we used 2 hosts", "we used two hosts", &none).is_empty());
        assert!(
            !lost("a 98 floor", "a floor", &none).is_empty(),
            "two digits IS a fact"
        );
    }

    /// A multi-line code fence is not an identifier -- only inline spans
    /// promise a name.
    #[test]
    fn only_inline_backticks_count_as_identifiers() {
        let ids = backticked("see `--bpe` and\n```\nnot this\n```\n");
        assert!(ids.contains("--bpe"));
        assert!(!ids.iter().any(|s| s.contains("not this")));
    }

    /// The baseline really comes from git, so the pass compares against
    /// what is COMMITTED rather than whatever is in the working tree.
    #[test]
    fn the_baseline_is_read_from_git() {
        let old = spec_at("HEAD");
        assert!(old.contains("INVARIANTS"), "HEAD:SPEC.md is readable");
        assert!(!must_keep(&old).is_empty(), "and yields a must-keep set");
    }

    /// The hatch parses a list, trims, and ignores blanks. The previous
    /// version of this test asserted `x.is_empty() || !x.is_empty()` --
    /// vacuously true, which is worse than no test at all (V79).
    #[test]
    fn the_allow_list_parses_a_comma_list() {
        let got = parse_allow("V42, T30 ,, `--flag`");
        assert_eq!(got.len(), 3);
        assert!(got.contains("V42") && got.contains("`--flag`"));
        assert!(parse_allow("").is_empty(), "no list means no exemptions");
    }

    /// An id and its digits are ONE fact: blanking ids is what makes
    /// `--allow-drop` able to allow anything at all.
    #[test]
    fn an_id_does_not_also_register_as_a_bare_number() {
        assert!(
            !numbers("cites V42").contains("42"),
            "the id owns its digits"
        );
        assert!(
            numbers("measured 42 turns").contains("42"),
            "prose does not"
        );
    }
}
