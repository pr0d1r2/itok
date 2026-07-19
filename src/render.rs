//! Human rendering -- the cosmetic table, free to evolve. The stable
//! machine contract (`--format json`) is a separate concern (V9, T3).

use crate::estimate::Estimate;

/// The unit every count carries: INPUT tokens -- what a file costs fed
/// INTO a model, not output/generation (V3). Self-documenting so an
/// agent needs no external knowledge.
const UNIT: &str = "itok";

/// A tier's method label and whether its number is a crude estimate.
/// The `~` marks only the dummy tier; bpe/exact are a real tokenizer's
/// true count and drop it (V3).
pub struct Method {
    pub label: &'static str,
    pub approximate: bool,
}

/// Dummy tier: bytes/4, crude -> carries the tilde.
pub const DUMMY: Method = Method {
    label: "bytes/4",
    approximate: true,
};

/// The bpe tier: o200k_base, a real tokenizer's true count -> no tilde.
pub const O200K: Method = Method {
    label: "o200k",
    approximate: false,
};

/// The exact tier: a local model's OWN tokenizer via ollama -- the true
/// count, no tilde (V3, V4, V22). Behind the `ollama` feature, since only
/// that backend reaches it.
#[cfg(feature = "ollama")]
pub const EXACT: Method = Method {
    label: "exact",
    approximate: false,
};

/// Compact human size: `37k`, `1M`. Integer-truncated -- precision is not
/// the point of `-h`, and the exact number is one flag away.
#[must_use]
pub fn human(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{}M", n.checked_div(1_000_000).unwrap_or(0))
    } else if n >= 1_000 {
        format!("{}k", n.checked_div(1_000).unwrap_or(0))
    } else {
        n.to_string()
    }
}

/// Total tokens, saturating -- a token counter never panics on overflow.
#[must_use]
pub fn total(ests: &[Estimate]) -> u64 {
    ests.iter().fold(0, |acc, e| acc.saturating_add(e.tokens))
}

/// How to render: `summarize` collapses to the total line, `human`
/// abbreviates counts. A struct, not two bool params -- the fn-params
/// bool limit is 1 (clippy.toml).
#[derive(Default)]
pub struct Style {
    pub summarize: bool,
    pub human: bool,
}

fn show(n: u64, human_flag: bool) -> String {
    if human_flag {
        human(n)
    } else {
        n.to_string()
    }
}

/// One count cell: `~166k itok` (dummy) or `166k itok` (bpe/exact).
fn cell(n: u64, method: &Method, human_flag: bool) -> String {
    let mark = if method.approximate { "~" } else { "" };
    format!("{mark}{} {UNIT}", show(n, human_flag))
}

/// A right-aligned count cell beside its label.
fn line(cell: &str, label: &str) -> String {
    format!("{cell:>16}  {label}\n")
}

/// The estimate report. The total names its method; every count carries
/// its unit and, for a crude tier, the `~` estimate marker (V3).
#[must_use]
pub fn report(ests: &[Estimate], style: &Style, method: &Method) -> String {
    let mut out = String::new();
    if !style.summarize {
        for e in ests {
            out.push_str(&line(&cell(e.tokens, method, style.human), &e.path));
        }
    }
    let total_label = format!("total ({})", method.label);
    out.push_str(&line(&cell(total(ests), method, style.human), &total_label));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn est(path: &str, tokens: u64) -> Estimate {
        Estimate {
            path: path.to_owned(),
            tokens,
        }
    }

    #[test]
    fn human_uses_k_and_m() {
        assert_eq!(human(0), "0");
        assert_eq!(human(999), "999");
        assert_eq!(human(37_000), "37k");
        assert_eq!(human(1_000_000), "1M");
    }

    #[test]
    fn total_sums_tokens() {
        assert_eq!(total(&[est("a", 10), est("b", 5)]), 15);
    }

    #[test]
    fn report_lists_files_names_method_and_unit() {
        let out = report(&[est("a.rs", 12)], &Style::default(), &DUMMY);
        assert!(out.contains("a.rs"));
        assert!(out.contains("itok"));
        assert!(out.contains("total (bytes/4)"));
    }

    #[test]
    fn the_dummy_tier_marks_its_number_approximate() {
        let out = report(&[est("a.rs", 12)], &Style::default(), &DUMMY);
        assert!(out.contains("~12 itok"), "expected tilde: {out:?}");
    }

    #[test]
    fn an_exact_tier_drops_the_tilde() {
        let exact = Method {
            label: "exact",
            approximate: false,
        };
        let out = report(&[est("a.rs", 12)], &Style::default(), &exact);
        assert!(out.contains("12 itok"));
        assert!(
            !out.contains("~"),
            "exact must not be tilde-marked: {out:?}"
        );
    }

    #[test]
    fn summarize_drops_the_per_file_lines() {
        let style = Style {
            summarize: true,
            human: false,
        };
        let out = report(&[est("a.rs", 12)], &style, &DUMMY);
        assert!(!out.contains("a.rs"));
        assert!(out.contains("total"));
    }
}
