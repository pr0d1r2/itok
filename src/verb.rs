//! Verb resolution with prefix-inference (V6): an unambiguous prefix
//! resolves silently, an ambiguous one errors with candidates, and a
//! non-prefix typo suggests the nearest verb but never runs it. Canonical
//! names are the full words; prefixes are convenience, never promised.
//!
//! Only READ-ONLY verbs are inferred. A mutating verb (none yet) is
//! excluded from this table and must be spelled in full, so a
//! fat-fingered prefix can never trigger a write.
//!
//! `edit_distance_1` is reimplemented here rather than imported from the
//! host's repo-guard -- zero host deps (V13), so this survives extraction.

/// The verbs itok implements. Add a variant + a row per new verb.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verb {
    Estimate,
    Doctor,
    Diff,
    Show,
    Log,
    Check,
    Fit,
    Trace,
    Top,
    Headroom,
    Calibrate,
    Cap,
}

/// Read-only verbs, the only ones prefix-inference resolves. `fit` selects
/// rather than reports but is still read-only (V20), so it belongs here.
pub(crate) const VERBS: &[(&str, Verb)] = &[
    ("estimate", Verb::Estimate),
    ("doctor", Verb::Doctor),
    ("diff", Verb::Diff),
    ("show", Verb::Show),
    ("log", Verb::Log),
    ("check", Verb::Check),
    ("fit", Verb::Fit),
    ("trace", Verb::Trace),
    ("top", Verb::Top),
    ("headroom", Verb::Headroom),
    ("calibrate", Verb::Calibrate),
    ("cap", Verb::Cap),
];

/// The outcome of resolving a verb token.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Resolution {
    Verb(Verb),
    Ambiguous(Vec<&'static str>),
    Unknown(Vec<&'static str>),
}

pub(crate) fn resolve(input: &str) -> Resolution {
    resolve_in(input, VERBS)
}

fn resolve_in(input: &str, verbs: &[(&'static str, Verb)]) -> Resolution {
    if let Some(&(_, v)) = verbs.iter().find(|(n, _)| *n == input) {
        return Resolution::Verb(v); // exact full name always wins
    }
    let hits: Vec<(&'static str, Verb)> = verbs
        .iter()
        .copied()
        .filter(|(n, _)| n.starts_with(input))
        .collect();
    match hits.as_slice() {
        [(_, v)] => Resolution::Verb(*v),
        [] => Resolution::Unknown(near(input, verbs)),
        _ => Resolution::Ambiguous(hits.iter().map(|(n, _)| *n).collect()),
    }
}

/// Verbs within edit-distance 1 of `input` -- the typo suggestions.
fn near(input: &str, verbs: &[(&'static str, Verb)]) -> Vec<&'static str> {
    verbs
        .iter()
        .filter(|(n, _)| edit_distance_1(input, n))
        .map(|(n, _)| *n)
        .collect()
}

fn edit_distance_1(a: &str, b: &str) -> bool {
    let (a, b): (Vec<char>, Vec<char>) =
        (a.chars().collect(), b.chars().collect());
    match a.len().abs_diff(b.len()) {
        0 => a.iter().zip(&b).filter(|(x, y)| x != y).count() == 1,
        1 => one_deletion_apart(&a, &b),
        _ => false,
    }
}

/// True when deleting one char from the longer word yields the shorter.
fn one_deletion_apart(a: &[char], b: &[char]) -> bool {
    let (long, short) = if a.len() > b.len() { (a, b) } else { (b, a) };
    (0..long.len()).any(|skip| {
        let kept = long.iter().enumerate().filter(|(i, _)| *i != skip);
        kept.map(|(_, c)| *c).eq(short.iter().copied())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `top` and `trace` both start with `t`, so the bare prefix must
    /// ERROR with candidates rather than silently picking one (V6). A new
    /// verb can make an old prefix ambiguous, which is exactly the case
    /// worth pinning.
    #[test]
    fn t_is_ambiguous_between_top_and_trace() {
        let got = match resolve("t") {
            Resolution::Ambiguous(mut c) => {
                c.sort_unstable();
                c
            }
            other => vec![format!("{other:?}").leak() as &str],
        };
        assert_eq!(got, vec!["top", "trace"]);
    }

    /// V91's naming rests on this: `headroom` was chosen over `free` --
    /// the better unix prior -- precisely because `fit` already owns an
    /// `f`, so `itok f` would have become AMBIGUOUS and broken a working
    /// prefix (V6). Both halves are pinned: `h` resolves, and `f` still
    /// does too.
    #[test]
    fn headroom_takes_h_without_costing_fit_its_f() {
        assert_eq!(resolve("h"), Resolution::Verb(Verb::Headroom));
        assert_eq!(resolve("f"), Resolution::Verb(Verb::Fit));
    }

    /// V6: `calibrate`, `check` and `cap` share a `c`, so the bare prefix
    /// must ERROR with candidates rather than silently picking one. Each
    /// verb name is specced in the interface section, so the collision is
    /// accepted and HANDLED -- what V6 forbids is the silent pick, not the
    /// ambiguity.
    #[test]
    fn c_is_ambiguous_across_the_three_c_verbs() {
        let got = match resolve("c") {
            Resolution::Ambiguous(mut c) => {
                c.sort_unstable();
                c
            }
            other => vec![format!("{other:?}").leak() as &str],
        };
        assert_eq!(got, vec!["calibrate", "cap", "check"]);
        // Longer prefixes still resolve, so the cost is one keystroke.
        assert_eq!(resolve("cal"), Resolution::Verb(Verb::Calibrate));
        assert_eq!(resolve("ch"), Resolution::Verb(Verb::Check));
        assert_eq!(resolve("cap"), Resolution::Verb(Verb::Cap));
    }

    /// The COST of adding `cap`, pinned rather than discovered later: `ca`
    /// used to resolve to `calibrate` and is now ambiguous. V6 promises
    /// canonical FULL names, never a particular prefix, so this is a
    /// legitimate change -- but it is a change, and an ambiguity that
    /// appeared silently would be exactly the surprise V6 forbids.
    #[test]
    fn adding_cap_made_ca_ambiguous_rather_than_a_silent_pick() {
        assert_eq!(
            resolve("ca"),
            Resolution::Ambiguous(vec!["calibrate", "cap"])
        );
    }

    /// Longer prefixes still resolve, so the ambiguity costs nothing.
    #[test]
    fn longer_prefixes_still_resolve() {
        assert_eq!(resolve("to"), Resolution::Verb(Verb::Top));
        assert_eq!(resolve("tr"), Resolution::Verb(Verb::Trace));
    }

    #[test]
    fn exact_name_resolves() {
        assert_eq!(resolve("estimate"), Resolution::Verb(Verb::Estimate));
    }

    #[test]
    fn unambiguous_prefixes_resolve_to_their_verbs() {
        assert_eq!(resolve("e"), Resolution::Verb(Verb::Estimate));
        assert_eq!(resolve("do"), Resolution::Verb(Verb::Doctor));
        assert_eq!(resolve("di"), Resolution::Verb(Verb::Diff));
        assert_eq!(resolve("f"), Resolution::Verb(Verb::Fit));
    }

    #[test]
    fn fit_resolves_by_full_name() {
        assert_eq!(resolve("fit"), Resolution::Verb(Verb::Fit));
    }

    #[test]
    fn a_shared_prefix_is_ambiguous() {
        // doctor and diff both start with d -- resolve must not guess.
        assert_eq!(resolve("d"), Resolution::Ambiguous(vec!["doctor", "diff"]));
    }

    #[test]
    fn any_prefix_resolves() {
        for p in ["e", "es", "est", "estim", "estimate"] {
            assert_eq!(resolve(p), Resolution::Verb(Verb::Estimate), "{p}");
        }
    }

    #[test]
    fn a_dropped_letter_suggests_the_nearest() {
        assert_eq!(resolve("esimate"), Resolution::Unknown(vec!["estimate"]));
    }

    #[test]
    fn a_one_letter_swap_suggests_the_nearest() {
        assert_eq!(resolve("estimatz"), Resolution::Unknown(vec!["estimate"]));
    }

    #[test]
    fn an_extra_letter_suggests_the_nearest() {
        // Input longer than the verb by one -- the other deletion branch.
        assert_eq!(resolve("estimatee"), Resolution::Unknown(vec!["estimate"]));
    }

    #[test]
    fn a_far_word_is_unknown_with_no_suggestion() {
        assert_eq!(resolve("xyzzy"), Resolution::Unknown(vec![]));
    }

    #[test]
    fn an_ambiguous_prefix_lists_candidates() {
        let table = [("estimate", Verb::Estimate), ("export", Verb::Estimate)];
        assert_eq!(
            resolve_in("e", &table),
            Resolution::Ambiguous(vec!["estimate", "export"])
        );
    }

    #[test]
    fn an_exact_name_beats_a_prefix_of_a_longer_verb() {
        let table =
            [("estimate", Verb::Estimate), ("estimatex", Verb::Estimate)];
        assert_eq!(
            resolve_in("estimate", &table),
            Resolution::Verb(Verb::Estimate)
        );
    }
}
