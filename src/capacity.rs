//! The capacity ladder: what a context window HOLDS, from the flags and
//! tables that can say so.
//!
//! Extracted rather than copied (V114). `headroom` owned this and `rate`
//! now needs the same answer; two ladders would drift, and then the badge
//! and `df` would disagree about the same session -- which is worse than
//! either being wrong alone, because the disagreement is what a reader
//! trusts least and neither number says which one moved.
//!
//! The NAMING TRAP `headroom` guards by hand applies here too: this is
//! CAPACITY, the size of the tank. What the model actually received on
//! the last turn is `Session::window()`, which is `used`. Both are called
//! "window" in ordinary speech and they are never the same quantity.

use std::path::Path;

/// Capacity from `--window`, else `--model` via `.context-models`.
///
/// Explicit `--window` WINS over the table (V18), and an unknown model is
/// an ERROR rather than a default, so a wrong capacity can never be
/// silently assumed (V11). `None` is the honest answer when nothing was
/// asked for: no flag, no model, no opinion (V92).
pub(crate) fn resolve(
    window: Option<&str>,
    model: Option<&str>,
    root: &Path,
) -> Result<Option<u64>, String> {
    if let Some(w) = window {
        return crate::units::parse(w).map(Some);
    }
    Ok(crate::models::tier(model, root)?.window)
}

/// The same ladder for a caller that was asked for a DISPLAY, not for a
/// capacity: an unresolvable model is no opinion rather than a failure.
///
/// `rate` reads its model from the harness payload, so the name arriving
/// here is whatever the harness happens to run today -- a model no
/// `.context-models` row mentions is the NORMAL case, not a usage error.
/// V11's hard failure protects a COUNT from a wrong tokenizer; there is
/// no count here, only a colour, and V109 already answers a missing
/// threshold by declining to have an opinion.
#[cfg_attr(not(feature = "session"), expect(dead_code))]
pub(crate) fn or_none(
    window: Option<&str>,
    model: Option<&str>,
    root: &Path,
) -> Option<u64> {
    resolve(window, model, root).ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_explicit_window_wins_over_everything() {
        let got = resolve(Some("40k"), Some("nonesuch"), Path::new("."));
        assert_eq!(got, Ok(Some(40_000)));
    }

    #[test]
    fn no_flag_and_no_model_is_no_opinion() {
        assert_eq!(resolve(None, None, Path::new(".")), Ok(None));
    }

    #[test]
    fn an_unknown_model_is_an_error_for_a_capacity() {
        let got = resolve(None, Some("nonesuch"), Path::new("."));
        assert!(got.is_err(), "expected V11's hard failure, got {got:?}");
    }

    #[test]
    fn an_unknown_model_is_merely_no_opinion_for_a_display() {
        assert_eq!(or_none(None, Some("nonesuch"), Path::new(".")), None);
    }

    #[test]
    fn an_unparsable_window_is_an_error_in_both_shapes() {
        assert!(resolve(Some("banana"), None, Path::new(".")).is_err());
        assert_eq!(or_none(Some("banana"), None, Path::new(".")), None);
    }
}
