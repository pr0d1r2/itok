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
pub(crate) fn measure(
    opts: &Opts,
    root: &Path,
) -> Result<Vec<Estimate>, String> {
    let ests: Vec<Estimate> = select(opts, root)?
        .iter()
        .filter_map(|f| {
            count(&root.join(f), opts.is_bpe()).map(|tokens| Estimate {
                path: f.clone(),
                tokens,
            })
        })
        .collect();
    Ok(cap(ests, opts.top))
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
pub(crate) fn select(opts: &Opts, root: &Path) -> Result<Vec<String>, String> {
    select_paths(&opts.paths, root)
}

/// The selection rule itself, over a bare path list -- `fit` has its own
/// options struct and would otherwise need a second copy (V64).
pub(crate) fn select_paths(
    paths: &[String],
    root: &Path,
) -> Result<Vec<String>, String> {
    if paths.is_empty() {
        return Ok(tracked(root));
    }
    for p in paths {
        usable(&root.join(p), p)?;
    }
    Ok(paths.to_vec())
}

/// An explicit path must be a readable FILE.
///
/// V8 keeps this a FLAT fileset -- no recursive walk -- so a directory is
/// not a small file, it is a category error. Both old behaviours lied:
/// `bytes()` returned the inode size, and the bpe tier's read failed and
/// the row was silently DROPPED, so `estimate --bpe src` reported 0 itok
/// and `fit --window 40k src` emitted `src` as something that fits. A zero
/// reads as a measurement (V47); B11d.
fn usable(full: &Path, shown: &str) -> Result<(), String> {
    let md = std::fs::metadata(full).map_err(|e| format!("{shown}: {e}"))?;
    if md.is_dir() {
        return Err(format!(
            "{shown} is a directory -- itok costs a flat fileset, so name \
             files, or omit paths for the git-tracked set"
        ));
    }
    Ok(())
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

    /// B11d/V47: a DIRECTORY is refused with its reason, never counted.
    /// Both old behaviours lied -- the dummy tier returned the inode size,
    /// and the bpe tier's read failed so the row was silently dropped and
    /// the total read 0 itok, which looks like a measurement of empty.
    #[test]
    fn a_directory_argument_is_refused_with_its_reason() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let err = select_paths(&["src".to_owned()], root)
            .err()
            .unwrap_or_default();
        assert!(err.contains("src"), "names the path: {err}");
        assert!(err.contains("directory"), "names the reason: {err}");
        assert!(err.contains("flat fileset"), "names the rule: {err}");
    }

    /// An unreadable path is named too, rather than dropped from the set.
    #[test]
    fn a_missing_path_is_named_not_dropped() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let err = select_paths(&["no/such/file".to_owned()], root)
            .err()
            .unwrap_or_default();
        assert!(err.contains("no/such/file"), "{err}");
    }

    /// Real files still pass, and the tracked default needs no check --
    /// git only lists files.
    #[test]
    fn files_and_the_tracked_default_still_select() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert_eq!(
            select_paths(&["Cargo.toml".to_owned()], root),
            Ok(vec!["Cargo.toml".to_owned()])
        );
        assert!(select_paths(&[], root).is_ok());
    }
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

    /// CHANGED by T83, deliberately: a named path that cannot be read is
    /// an ERROR, not a silent omission. This test previously asserted the
    /// empty result -- which is the same defect as B11d one field over,
    /// since a total computed from fewer files than you named understates
    /// without saying so (V47).
    #[test]
    fn a_missing_named_file_is_an_error_not_an_omission() {
        let opts = Opts {
            paths: vec!["no-such-itok-file-xyz".to_owned()],
            ..Default::default()
        };
        let err = measure(&opts, Path::new(".")).err().unwrap_or_default();
        assert!(err.contains("no-such-itok-file-xyz"), "{err}");
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
