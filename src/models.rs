//! `.context-models`: the model -> encoding table (V11). `--model X` names
//! a model; the table says which tokenizer encodes it. An unknown model --
//! or a missing table, or a row naming an encoding this build has no
//! tokenizer for -- FAILS; itok never silently falls back to a default BPE,
//! because a wrong tokenizer is the V2 near-collision (a number that looks
//! true but counts by the wrong vocabulary). A new model is a reviewed
//! row, not a guess.
//!
//! Mirrors `.context-limits` (checkcmd): `model  encoding` per line, `#`
//! comments and blank lines skipped, malformed lines dropped. One offline
//! encoding ships today (`o200k`, behind the `bpe` feature); the network
//! tiers (T6 anthropic, T17 ollama) are their own rungs. T14 extends each
//! row with a window column, reusing this parser.

use crate::render::Method;
#[cfg(feature = "bpe")]
use crate::render::O200K;
use crate::units;
use std::path::Path;

const MODELS: &str = ".context-models";

/// A resolved model row: its encoding tier and, optionally, its context
/// window. The window is the model-fit substrate (V36) -- a static ceiling
/// (e.g. `qwen3-coder:30b` = 256k) read with no network.
pub(crate) struct Model {
    pub(crate) encoding: &'static Method,
    pub(crate) window: Option<u64>,
    pub(crate) rates: Option<Rates>,
}

/// What a million tokens cost, split three ways (V61).
///
/// THREE rates, because V61's own clause requires it: cache-read bills at
/// a fraction of fresh input and cache-creation at a premium, so a single
/// rate is a wrong number wearing a currency.
///
/// The UNIT is whatever the file declares -- itok bundles no price list
/// and names no currency, because prices change, vary by contract, and a
/// built-in would go stale into a confident lie. Integers, so the
/// arithmetic that follows carries no float and no rounding anyone has to
/// argue about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rates {
    /// Fresh input, per million tokens.
    pub(crate) input: u64,
    /// Cache reads, per million tokens.
    pub(crate) cache_read: u64,
    /// Cache writes, per million tokens.
    pub(crate) cache_write: u64,
}

/// A run's tier + window as one `Model`: a named model resolves both from
/// the table (V11 unknown => fail); `None` takes the built-in tier and no
/// window.
pub(crate) fn tier(model: Option<&str>, root: &Path) -> Result<Model, String> {
    match model {
        Some(name) => resolve(root, name),
        None => Ok(Model {
            encoding: default(),
            window: None,
            rates: None,
        }),
    }
}

/// The default tier with no `--model`: o200k when the bpe tokenizer is
/// compiled, bytes/4 otherwise. cfg, not a runtime `if`, so the untaken
/// arm is not compiled (mirrors checkcmd::method).
fn default() -> &'static Method {
    #[cfg(feature = "bpe")]
    {
        &O200K
    }
    #[cfg(not(feature = "bpe"))]
    {
        &crate::render::DUMMY
    }
}

/// Resolve `--model NAME` against the repo's `.context-models`. A missing
/// table is not an empty table: it means NAME is unknown (V11, no silent
/// fallback).
pub(crate) fn resolve(root: &Path, name: &str) -> Result<Model, String> {
    let text = std::fs::read_to_string(root.join(MODELS))
        .map_err(|_| format!("unknown model '{name}': no {MODELS}"))?;
    find(&text, name)
}

/// Look up NAME's row. Absent model, an encoding this build cannot honor,
/// or a malformed window are all errors -- never a silent wrong value.
/// A row's fields: model, encoding, an optional window, optional rates.
type Row = (String, String, Option<String>, Option<String>);

/// A model id stripped of its VARIANT suffix: `claude-opus-5[1m]` becomes
/// `claude-opus-5`.
///
/// The harness names a context-window variant with a bracketed tag, and
/// that id is what reaches us on the statusline payload. It is a REAL
/// distinction -- a `[1m]` run holds a million tokens where the base run
/// does not -- so the suffix is never discarded, only fallen back from.
pub(crate) fn base_id(name: &str) -> &str {
    match name.split_once('[') {
        Some((base, tail)) if tail.ends_with(']') && !base.is_empty() => base,
        _ => name,
    }
}

/// NAME's row: the FULL id first, the base id only as a fallback.
///
/// Exact-first is the load-bearing half. A table that names the variant
/// outright is stating a window FOR that variant and has to win; reaching
/// for the base first would answer a question about a million-token run
/// with a number written for the smaller one. The fallback is for the
/// ordinary table that lists base ids only, where "no row" costs the
/// caller its whole capacity rather than a little precision.
fn row_for(rows: &[Row], name: &str) -> Option<Row> {
    rows.iter()
        .find(|(m, _, _, _)| m == name)
        .or_else(|| rows.iter().find(|(m, _, _, _)| m == base_id(name)))
        .cloned()
}

fn find(text: &str, name: &str) -> Result<Model, String> {
    // Every row is parsed BEFORE the lookup, so an unreadable row fails
    // even when the requested model is found earlier in the file (V88): the
    // author wrote that row expecting it to work.
    let (_, enc, win, rate) = row_for(&rows(text)?, name).ok_or_else(|| {
        format!("unknown model '{name}'; add a row to {MODELS}")
    })?;
    // Window BEFORE encoding, and the order is load-bearing: a malformed
    // window means the ROW is unreadable (V88), while an unhonorable
    // encoding only means THIS BUILD cannot serve it. The file's error
    // should win, or `--no-default-features` reports a build limitation for
    // a row that is simply wrong.
    let window = window_of(win.as_deref(), name)?;
    let rates = rates_of(rate.as_deref(), name)?;
    Ok(Model {
        encoding: encoding(&enc)?,
        window,
        rates,
    })
}

/// A row's optional window column, parsed with the one unit grammar (V18).
fn window_of(win: Option<&str>, name: &str) -> Result<Option<u64>, String> {
    match win {
        Some(w) => units::parse(w)
            .map(Some)
            .map_err(|e| format!("bad window for '{name}': {e}")),
        None => Ok(None),
    }
}

/// The well-formed rows: `model  encoding [window]`, skipping comments,
/// blanks, and any line without at least model + encoding. The window (a
/// unit-suffixed count, T14) is optional; a row can name an encoding alone.
fn rows(text: &str) -> Result<Vec<Row>, String> {
    text.lines()
        .enumerate()
        .map(|(i, l)| (i.saturating_add(1), l.trim()))
        .filter(|(_, l)| !l.is_empty() && !l.starts_with('#'))
        .map(|(n, l)| row(l).ok_or_else(|| row_err(n, l)))
        .collect()
}

fn row(line: &str) -> Option<Row> {
    let mut p = line.split_whitespace();
    let model = p.next()?.to_owned();
    let enc = p.next()?.to_owned();
    let window = p.next().map(str::to_owned);
    Some((model, enc, window, p.next().map(str::to_owned)))
}

/// A row's optional rate column: `in/cr/cc`, three integers.
///
/// A malformed one is an ERROR, never an absent one (V11/B7): a row whose
/// author wrote rates expecting them to work must not be read as a row
/// with none, because the difference between "no money column" and "the
/// wrong money column" is the whole of V61.
fn rates_of(rate: Option<&str>, name: &str) -> Result<Option<Rates>, String> {
    let Some(text) = rate else { return Ok(None) };
    let mut parts = text.split('/').map(str::parse::<u64>);
    let (Some(Ok(input)), Some(Ok(read)), Some(Ok(write)), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(format!(
            "bad rates for '{name}': expected `in/cr/cc`, three whole \
             numbers per million tokens, got `{text}`"
        ));
    };
    Ok(Some(Rates {
        input,
        cache_read: read,
        cache_write: write,
    }))
}

/// Names the FILE, the LINE, and what was expected (V88). The sibling
/// registry silently dropped such a row for a day (B7); a model row that
/// vanished would instead make `--model` report "unknown model" for a name
/// the author can SEE in the file -- a worse kind of confusing.
fn row_err(line: usize, text: &str) -> String {
    format!(
        "{MODELS}:{line}: expected \
         `<model> <encoding> [window] [in/cr/cc]`, got `{text}`"
    )
}

/// Map an encoding name to its tier. `o200k` needs the bpe tokenizer, so
/// an offline (`--no-default-features`) build cannot honor it and says so
/// rather than counting by bytes/4 under an `o200k` label (V3).
fn encoding(name: &str) -> Result<&'static Method, String> {
    #[cfg(feature = "bpe")]
    if name == "o200k" {
        return Ok(&O200K);
    }
    Err(format!("unknown encoding '{name}' in {MODELS}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // gpt-4o carries a window (T14); claude-x is encoding-only (window
    // optional, back-compatible with the T7 two-field row).
    const TABLE: &str = "# models\n\ngpt-4o   o200k  128k  300/30/375\n\
     claude-x o200k\n";

    /// The harness names a context-window variant with a bracketed tag,
    /// and that id is what lands on the statusline payload.
    #[test]
    fn a_bracketed_variant_reduces_to_its_base_id() {
        assert_eq!(base_id("claude-opus-5[1m]"), "claude-opus-5");
        assert_eq!(base_id("claude-opus-4-6[1m]"), "claude-opus-4-6");
    }

    /// A plain id is its own base -- the common case must not be touched.
    #[test]
    fn an_unsuffixed_id_is_left_alone() {
        assert_eq!(base_id("claude-opus-5"), "claude-opus-5");
        assert_eq!(base_id(""), "");
    }

    /// Only a WELL-FORMED trailing tag counts. A stray bracket is part of
    /// the name as far as this is concerned: trimming it would invent a
    /// model id the table never held, and V11 wants an unknown model to
    /// stay unknown rather than resolve to something adjacent.
    #[test]
    fn a_malformed_bracket_is_not_a_variant_tag() {
        assert_eq!(base_id("claude-opus-5[1m"), "claude-opus-5[1m");
        assert_eq!(base_id("[1m]"), "[1m]");
        assert_eq!(base_id("claude[1m]extra"), "claude[1m]extra");
    }

    /// The `.context-models` half of the same fallback. Tested through
    /// `row_for` rather than `find` for the reason given below: `find`
    /// resolves the encoding, which a `--no-default-features` build cannot
    /// do for an `o200k` row.
    #[test]
    fn a_variant_falls_back_to_the_base_row() {
        let rows = rows(TABLE).unwrap_or_default();
        let got = row_for(&rows, "claude-x[1m]");
        assert_eq!(got.map(|(m, _, _, _)| m), Some("claude-x".to_owned()));
    }

    /// ...and a row naming the variant beats the base row, because the
    /// windows genuinely differ.
    #[test]
    fn a_row_for_the_variant_wins_over_the_base_row() {
        let table = "claude-x o200k 200k\nclaude-x[1m] o200k 1m\n";
        let rows = rows(table).unwrap_or_default();
        let got = row_for(&rows, "claude-x[1m]");
        assert_eq!(got.and_then(|(_, _, w, _)| w), Some("1m".to_owned()));
    }

    /// The fallback must not conjure a base that was never listed.
    #[test]
    fn an_unlisted_model_has_no_row_suffix_or_not() {
        let rows = rows(TABLE).unwrap_or_default();
        assert!(row_for(&rows, "nonesuch[1m]").is_none());
    }

    #[test]
    fn rows_skip_comments_and_blanks() {
        let got: Vec<_> = rows(TABLE).unwrap_or_default();
        let names: Vec<&str> =
            got.iter().map(|(m, _, _, _)| m.as_str()).collect();
        assert_eq!(names, vec!["gpt-4o", "claude-x"]);
    }

    /// V61: three rates, because cache reads bill at a fraction and cache
    /// writes at a premium. One rate would be a wrong number wearing a
    /// currency.
    ///
    /// Tested through `rates_of` rather than `find`, so it runs in EVERY
    /// build: `find` resolves the encoding first, and a table naming
    /// `o200k` is unreadable under `--no-default-features` for a reason
    /// that has nothing to do with the rate column.
    #[test]
    fn a_rate_column_carries_the_cache_split() {
        let got = rates_of(Some("300/30/375"), "m").ok().flatten();
        assert_eq!(
            got,
            Some(Rates {
                input: 300,
                cache_read: 30,
                cache_write: 375,
            })
        );
    }

    /// A row with no rates has no money, and that is not zero (V47/V61):
    /// a guessed price is the thing V61 exists to refuse.
    #[test]
    fn a_row_without_rates_has_no_money() {
        assert_eq!(rates_of(None, "m"), Ok(None));
    }

    /// B7's rule on a new column: a malformed rate is an ERROR, never an
    /// absent one. The author wrote it expecting it to work, and "no money
    /// column" and "the wrong money column" are the two answers V61 exists
    /// to keep apart.
    #[test]
    fn a_malformed_rate_fails_rather_than_reading_as_absent() {
        for bad in ["300/30", "300/30/375/9", "300/x/375", "free", ""] {
            assert!(
                rates_of(Some(bad), "m").is_err(),
                "`{bad}` parsed instead of failing"
            );
        }
    }

    /// And the column reaches `Model`, which is what the caller reads.
    #[cfg(feature = "bpe")]
    #[test]
    fn a_resolved_model_carries_its_rates() {
        let got = find(TABLE, "gpt-4o").ok().and_then(|m| m.rates);
        assert_eq!(got.map(|r| r.cache_read), Some(30));
    }

    #[test]
    fn a_line_missing_its_encoding_is_dropped() {
        // B7's rule on this registry: an incomplete row FAILS, naming the
        // file, the line, and the expected form -- it is not skipped (V88).
        let msg = rows("gpt-4o\n").err().unwrap_or_default();
        assert!(msg.contains(".context-models:1:"), "{msg}");
        assert!(msg.contains("expected"), "{msg}");
    }

    #[test]
    fn an_unknown_model_is_an_error() {
        // V11: not in the table => FAIL, never a silent default.
        assert!(find(TABLE, "gpt-5").is_err());
    }

    #[test]
    fn a_bad_encoding_is_an_error() {
        // A listed model whose encoding itok does not know still fails,
        // naming the offending encoding.
        let e = find("weird  koi8\n", "weird").err().unwrap_or_default();
        assert!(e.contains("koi8"), "{e}");
    }

    #[test]
    fn a_row_without_a_window_has_none() {
        // Back-compat: a two-field row resolves, window absent (V18).
        assert_eq!(find(TABLE, "claude-x").ok().and_then(|m| m.window), None);
    }

    #[test]
    fn a_bad_window_is_an_error() {
        // V11 spirit for the window column: a malformed count fails loudly
        // rather than resolving to a silent wrong ceiling.
        let e = find("m  o200k  huge\n", "m").err().unwrap_or_default();
        assert!(e.contains("window"), "{e}");
    }

    #[cfg(feature = "bpe")]
    #[test]
    fn a_listed_model_resolves_its_encoding_and_window() {
        // V18: `--model` carries both the encoding and the window.
        let m = find(TABLE, "gpt-4o").ok().unwrap_or(Model {
            encoding: &crate::render::DUMMY,
            window: None,
            rates: None,
        });
        assert_eq!(m.encoding.label(), "o200k");
        assert!(!m.encoding.approximate);
        assert_eq!(m.window, Some(128_000));
    }

    #[cfg(feature = "bpe")]
    #[test]
    fn tier_carries_the_models_window() {
        // The window flows through `tier`, the doctor entry point (V18).
        let dir = std::env::temp_dir()
            .join(format!("itok-t14-tier-{}", std::process::id()));
        std::fs::create_dir_all(&dir).ok();
        std::fs::write(dir.join(".context-models"), "qwen  o200k  256k\n").ok();
        let window = tier(Some("qwen"), &dir).ok().and_then(|m| m.window);
        assert_eq!(window, Some(256_000));
    }

    #[test]
    fn no_model_takes_the_default_tier() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        assert!(tier(None, &root).is_ok());
    }

    #[test]
    fn a_named_unknown_model_makes_tier_fail() {
        // V11 at the tier entry point: an unresolvable model is an error,
        // never a silent default.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        assert!(tier(Some("gpt-4o"), &root).is_err());
    }

    #[test]
    fn a_missing_table_means_the_model_is_unknown() {
        // src/ has no .context-models; resolve treats that as unknown, not
        // as an empty pass (V11).
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        assert!(resolve(&root, "gpt-4o").is_err());
    }

    #[cfg(not(feature = "bpe"))]
    #[test]
    fn o200k_needs_bpe() {
        // Offline core has no real tokenizer, so it cannot honor o200k.
        assert!(encoding("o200k").is_err());
    }
}
