//! Human rendering -- the cosmetic table, free to evolve. The stable
//! machine contract (`--format json`) is a separate concern (V9, T3).

use crate::estimate::Estimate;
use std::borrow::Cow;

/// The unit every count carries: INPUT tokens -- what a file costs fed
/// INTO a model, not output/generation (V3). Self-documenting so an
/// agent needs no external knowledge.
const UNIT: &str = "itok";

/// A tier's method label and whether its number is a crude estimate.
/// The `~` marks only the dummy tier; bpe/exact are a real tokenizer's
/// true count and drop it (V3).
///
/// Split in two because the local tiers ARE their names -- `bytes/4` is
/// the whole story -- while the remote tier's name is incomplete without
/// saying which tokenizer produced the count (V101).
///
/// Keeping `tier` separate from the rendered label is what lets json hold
/// its contract by CONSTRUCTION rather than by discipline: `method` stays
/// the bare tier a parser already matches on, and the endpoint arrives as
/// its own field (V9).
pub struct Method {
    /// The tier's stable name -- json's `method` value.
    pub tier: &'static str,
    /// Where the count came from, when the tier alone does not say: the
    /// resolved `model@host` of the remote tier. `None` for the local
    /// tiers, which need no qualifier.
    pub endpoint: Option<Cow<'static, str>>,
    pub approximate: bool,
}

impl Method {
    /// The human label: the tier, qualified by its endpoint when it has
    /// one (V3 -- the number says what it is).
    #[must_use]
    pub fn label(&self) -> Cow<'_, str> {
        match &self.endpoint {
            Some(e) => Cow::Owned(format!("{} via {e}", self.tier)),
            None => Cow::Borrowed(self.tier),
        }
    }
}

/// Dummy tier: bytes/4, crude -> carries the tilde.
pub const DUMMY: Method = Method {
    tier: "bytes/4",
    endpoint: None,
    approximate: true,
};

/// The bpe tier: o200k_base, a real tokenizer's true count -> no tilde.
pub const O200K: Method = Method {
    tier: "o200k",
    endpoint: None,
    approximate: false,
};

/// The exact tier: a local model's OWN tokenizer via ollama -- the true
/// count, no tilde (V3, V4, V22). Behind the `ollama` feature, since only
/// that backend reaches it.
/// The exact tier, NAMING the tokenizer that produced the count.
///
/// `exact` on its own was a claim of truth that did not say truth of
/// WHAT: two different models on two different hosts rendered the same
/// four characters. B10 is what that costs -- a bare host silently
/// resolved to localhost, and had localhost been serving a different
/// model the answer would have been a confident `(exact)` count from the
/// wrong tokenizer (V3/V101).
///
/// Names the MODEL as well as the host, because the model IS the
/// tokenizer. With no `--model`, the host's first served model is chosen
/// for you, which is a silent pick this makes visible.
#[cfg(feature = "ollama")]
#[must_use]
pub fn exact_via(model: &str, base: &str) -> Method {
    Method {
        tier: "exact",
        endpoint: Some(Cow::Owned(format!("{model}@{}", short(base)))),
        approximate: false,
    }
}

/// A base URL with a bare `http://` dropped -- it is the default and adds
/// nothing. `https://` SURVIVES, because there it carries information: it
/// says the endpoint is behind TLS (V25's scheme slot).
#[cfg(feature = "ollama")]
fn short(base: &str) -> &str {
    base.strip_prefix("http://").unwrap_or(base)
}

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
    let total_label = format!("total ({})", method.label());
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

    /// V101/V3: the remote label names the tokenizer that produced the
    /// count -- model AND host. `exact` alone claimed truth without saying
    /// truth OF WHAT, which is what let B10 print a confident number from
    /// an unintended endpoint.
    #[cfg(feature = "ollama")]
    #[test]
    fn the_exact_label_names_its_endpoint() {
        let m = exact_via("gpt-oss:20b", "http://192.168.0.181:11434");
        assert_eq!(m.label(), "exact via gpt-oss:20b@192.168.0.181:11434");
        assert!(!m.approximate, "a true count keeps no tilde (V3)");
    }

    /// The property that makes a wrong tokenizer VISIBLE rather than
    /// merely named: two endpoints must not render alike.
    #[cfg(feature = "ollama")]
    #[test]
    fn two_endpoints_render_differently() {
        let a = exact_via("gpt-oss:20b", "http://192.168.0.181:11434");
        let b = exact_via("gpt-oss:20b", "http://localhost:11434");
        let c = exact_via("devstral:latest", "http://192.168.0.181:11434");
        assert_ne!(a.label(), b.label(), "a different HOST must be visible");
        assert_ne!(a.label(), c.label(), "a different MODEL must be visible");
    }

    /// A bare `http://` is the default and adds nothing; `https://`
    /// survives because there the scheme says the endpoint is behind TLS.
    #[cfg(feature = "ollama")]
    #[test]
    fn https_survives_while_plain_http_is_dropped() {
        assert!(exact_via("m", "http://h:1").label().ends_with("m@h:1"));
        assert!(exact_via("m", "https://h:1")
            .label()
            .ends_with("https://h:1"));
    }

    /// No collateral: the local tiers are their own names, byte for byte.
    /// A change here would silently rewrite every other verb's output.
    #[test]
    fn the_local_tier_labels_are_unchanged() {
        assert_eq!(DUMMY.label(), "bytes/4");
        assert_eq!(O200K.label(), "o200k");
        assert_eq!(DUMMY.endpoint, None, "a local tier needs no qualifier");
    }

    #[test]
    fn the_dummy_tier_marks_its_number_approximate() {
        let out = report(&[est("a.rs", 12)], &Style::default(), &DUMMY);
        assert!(out.contains("~12 itok"), "expected tilde: {out:?}");
    }

    #[test]
    fn an_exact_tier_drops_the_tilde() {
        let exact = Method {
            tier: "exact",
            endpoint: None,
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
