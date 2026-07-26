//! Path patterns for `.context-policy` (V57).
//!
//! V57 DEFINES these semantics and this is where they live. It used to say
//! the registry "reuses the glob semantics of the existing registries",
//! which it could not: `.context-limits` matches exact paths and
//! `.context-models` matches names, so there was nothing to reuse. The
//! rule was amended in the commit that added this file, because a rule and
//! the code that implements it land together (V105).
//!
//! One definition, here, which the siblings may adopt -- rather than a
//! second one grown quietly in another file (V64).
//!
//! Three wildcards, the ones every developer already knows from the shell
//! and from `.gitignore` (V1: the convention costs nothing to invoke):
//!
//!   `?`   one character, never `/`
//!   `*`   any run of characters WITHIN one path segment, never `/`
//!   `**`  any run of SEGMENTS, including none
//!
//! `*` stopping at `/` is the load-bearing half. A `*` that crossed
//! separators would make `src/*` silently mean `src/**`, so a budget
//! written for one directory would quietly govern the whole tree -- the
//! near-collision V2 warns about, in a config file where the mistake is
//! invisible until something is denied.
//!
//! Pure and total: no filesystem, no allocation beyond the split, and no
//! pattern is invalid. An unmatched `[` is a literal `[`, because a config
//! parser that can reject a pattern needs a rule for WHICH patterns are
//! legal, and that rule is a second grammar to learn (V57's own point).

/// Whether `path` matches `pattern`.
///
/// Both are split on `/`, so matching is per SEGMENT and a wildcard can
/// never leak across one unless it is `**`.
#[must_use]
pub(crate) fn matches(pattern: &str, path: &str) -> bool {
    let pat: Vec<&str> = pattern.split('/').collect();
    let seg: Vec<&str> = path.split('/').collect();
    segments(&pat, &seg)
}

/// Segment-wise match. `**` is the only pattern that consumes more (or
/// fewer) than one segment.
fn segments(pat: &[&str], path: &[&str]) -> bool {
    match pat.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => any_split(rest, path),
        Some((p, rest)) => path
            .split_first()
            .is_some_and(|(s, srest)| chars(p, s) && segments(rest, srest)),
    }
}

/// `**` matches zero or more segments, so try every remaining suffix. The
/// zero case is why `src/**/*.rs` also matches `src/a.rs`.
fn any_split(rest: &[&str], path: &[&str]) -> bool {
    (0..=path.len()).any(|i| segments(rest, path.get(i..).unwrap_or_default()))
}

/// Within one segment: `*` any run, `?` exactly one, everything else
/// literal.
fn chars(pattern: &str, seg: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let s: Vec<char> = seg.chars().collect();
    wild(&p, &s)
}

fn wild(p: &[char], s: &[char]) -> bool {
    match p.split_first() {
        None => s.is_empty(),
        Some((&'*', rest)) => {
            (0..=s.len()).any(|i| wild(rest, s.get(i..).unwrap_or_default()))
        }
        Some((&'?', rest)) => {
            s.split_first().is_some_and(|(_, t)| wild(rest, t))
        }
        Some((c, rest)) => s
            .split_first()
            .is_some_and(|(f, t)| f == c && wild(rest, t)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pattern with no wildcard is an exact path -- the behaviour
    /// `.context-limits` already has, so adopting these semantics there
    /// later would change nothing for its existing rows.
    #[test]
    fn a_literal_pattern_is_an_exact_path() {
        assert!(matches("SPEC.md", "SPEC.md"));
        assert!(!matches("SPEC.md", "src/SPEC.md"));
        assert!(!matches("SPEC.md", "SPEC.md.bak"));
    }

    /// THE load-bearing rule: `*` stops at a separator. Without it `src/*`
    /// silently means `src/**`, and a budget written for one directory
    /// governs the whole tree (V2).
    #[test]
    fn a_star_never_crosses_a_separator() {
        assert!(matches("src/*.rs", "src/a.rs"));
        assert!(!matches("src/*.rs", "src/session/mod.rs"));
        assert!(matches("src/*", "src/session"));
        assert!(!matches("src/*", "src/session/mod.rs"));
    }

    /// `**` crosses segments, and matches NONE of them too -- so
    /// `src/**/*.rs` covers a file sitting directly in `src/`.
    #[test]
    fn a_double_star_crosses_segments_including_zero() {
        assert!(matches("src/**/*.rs", "src/session/mod.rs"));
        assert!(matches("src/**/*.rs", "src/a.rs"), "zero segments");
        assert!(matches("**", "anything/at/all"));
        assert!(matches("**/*.rs", "a.rs"));
        assert!(!matches("src/**/*.rs", "tests/a.rs"));
    }

    /// `?` is exactly one character, and it does not cross a separator
    /// either.
    #[test]
    fn a_question_mark_is_one_character() {
        assert!(matches("a?.rs", "ab.rs"));
        assert!(!matches("a?.rs", "abc.rs"), "one, not many");
        assert!(!matches("a?.rs", "a.rs"), "one, not zero");
        assert!(!matches("a?c", "a/c"), "never a separator");
    }

    /// Every pattern is legal, so the parser needs no second grammar for
    /// what a pattern may contain. Unmatched punctuation is a literal.
    #[test]
    fn no_pattern_is_invalid() {
        assert!(matches("a[b.rs", "a[b.rs"));
        assert!(matches("", ""));
        assert!(!matches("", "a"));
        assert!(!matches("a", ""));
    }

    /// Wildcards compose without blowing up on a pathological pattern --
    /// the naive backtracker's failure case, kept small enough to stay
    /// linear in practice.
    #[test]
    fn stacked_wildcards_still_terminate() {
        assert!(matches("**/**/*.rs", "a/b/c/d.rs"));
        assert!(matches("*/*/*", "a/b/c"));
        assert!(!matches("*/*/*", "a/b"));
    }
}
