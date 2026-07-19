//! Dummy tier + file selection into estimates. The dummy tier is ~4
//! chars per token, the standard rule of thumb: zero-dep, instant (V4).
//! The word proxy joins at doctor (T13), where the dummy-vs-bpe spread
//! becomes the confidence signal; estimate reports one honest number and
//! always names its method (V3).
//!
//! Division uses checked_div, not `/`: arithmetic_side_effects is denied
//! crate-wide because integer ops can panic, and a token counter must
//! never be the thing that crashes.

use crate::args::Opts;
use crate::walk::{bytes, tracked};
use std::path::Path;

/// A file's estimated token cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Estimate {
    pub path: String,
    pub tokens: u64,
}

/// Dummy token estimate from a byte count.
#[must_use]
pub fn dummy(bytes: u64) -> u64 {
    bytes.checked_div(4).unwrap_or(0)
}

/// Estimate each selected file, biggest-first, capped by `--top`.
#[must_use]
pub(crate) fn measure(opts: &Opts, root: &Path) -> Vec<Estimate> {
    let ests: Vec<Estimate> = select(opts, root)
        .iter()
        .filter_map(|f| {
            count(&root.join(f), opts.is_bpe()).map(|tokens| Estimate {
                path: f.clone(),
                tokens,
            })
        })
        .collect();
    cap(ests, opts.top)
}

/// A file's token count on the selected tier. The dummy tier needs only
/// the byte size; bpe needs the file's text. Shared with diff's
/// "working tree" side (T8).
pub(crate) fn count(path: &Path, bpe: bool) -> Option<u64> {
    if bpe {
        bpe_count(path)
    } else {
        bytes(path).map(dummy)
    }
}

#[cfg(feature = "bpe")]
fn bpe_count(path: &Path) -> Option<u64> {
    std::fs::read_to_string(path)
        .ok()
        .map(|t| crate::bpe::count(&t))
}

#[cfg(not(feature = "bpe"))]
fn bpe_count(_path: &Path) -> Option<u64> {
    None
}

/// Sort biggest-first, then keep at most `top`. Pure -- proptested.
pub(crate) fn cap(
    mut ests: Vec<Estimate>,
    top: Option<usize>,
) -> Vec<Estimate> {
    ests.sort_by_key(|e| std::cmp::Reverse(e.tokens));
    if let Some(n) = top {
        ests.truncate(n);
    }
    ests
}

/// Explicit paths win; otherwise the git-tracked set (V8).
pub(crate) fn select(opts: &Opts, root: &Path) -> Vec<String> {
    if opts.paths.is_empty() {
        tracked(root)
    } else {
        opts.paths.clone()
    }
}

/// Files whose estimate exceeds `budget` -- the `--budget` breaches (V16).
/// No budget means no breaches (estimate stays report-only, V5).
#[must_use]
pub(crate) fn over_budget(
    ests: &[Estimate],
    budget: Option<u64>,
) -> Vec<&Estimate> {
    match budget {
        None => Vec::new(),
        Some(n) => ests.iter().filter(|e| e.tokens > n).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn four_bytes_is_one_token() {
        assert_eq!(dummy(4), 1);
    }

    #[test]
    fn an_empty_file_is_zero() {
        assert_eq!(dummy(0), 0);
    }

    #[test]
    fn it_is_a_quarter_of_the_bytes() {
        assert_eq!(dummy(1000), 250);
    }

    #[test]
    fn a_missing_file_is_skipped() {
        let opts = Opts {
            paths: vec!["no-such-itok-file-xyz".to_owned()],
            ..Default::default()
        };
        assert!(measure(&opts, Path::new(".")).is_empty());
    }

    #[test]
    fn no_budget_flags_nothing() {
        let all = ests(&[10, 20, 30]);
        assert!(over_budget(&all, None).is_empty());
    }

    fn ests(toks: &[u64]) -> Vec<Estimate> {
        toks.iter()
            .enumerate()
            .map(|(i, t)| Estimate {
                path: format!("f{i}"),
                tokens: *t,
            })
            .collect()
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        /// `--top N` never yields more than N rows.
        #[test]
        fn cap_never_exceeds_top(
            toks in prop::collection::vec(0u64..1000, 0..20),
            n in 0usize..25,
        ) {
            prop_assert!(cap(ests(&toks), Some(n)).len() <= n);
        }

        /// Biggest first, always.
        #[test]
        fn cap_sorts_descending(toks in prop::collection::vec(0u64..1000, 1..20)) {
            let out = cap(ests(&toks), None);
            let sorted = out
                .windows(2)
                .all(|w| matches!(w, [a, b] if a.tokens >= b.tokens));
            prop_assert!(sorted);
        }

        /// A breach is only ever a file strictly over budget.
        #[test]
        fn over_budget_only_flags_files_above(
            toks in prop::collection::vec(0u64..1000, 0..20),
            n in 0u64..1000,
        ) {
            let all = ests(&toks);
            let flagged = over_budget(&all, Some(n));
            prop_assert!(flagged.iter().all(|e| e.tokens > n));
        }
    }
}
