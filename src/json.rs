//! The stable machine contract: one JSON object per file (JSONL), the
//! plumbing half of the porcelain/plumbing split (V9). Hand-written --
//! zero deps, no serde (V13). Fields carry V3's intent: `unit` is
//! input_tokens, `estimated` flags the crude tier (the json form of the
//! human tilde). Numbers stay numeric; no tilde ever appears here.

use crate::estimate::Estimate;
use crate::render::Method;

/// The unit, spelled out for machines (the human table uses `itok`).
pub(crate) const UNIT: &str = "input_tokens";

/// One JSONL object for a file's estimate.
#[must_use]
pub fn object(est: &Estimate, method: &Method) -> String {
    format!(
        "{{\"path\":\"{}\",\"tokens\":{},\"unit\":\"{UNIT}\",\
         \"estimated\":{},\"method\":\"{}\"{}}}",
        escape(&est.path),
        est.tokens,
        method.approximate,
        escape(method.tier),
        endpoint(method),
    )
}

/// The remote tier's endpoint, as its OWN field.
///
/// `method` keeps the bare tier a parser already matches on -- folding
/// `via model@host` into it would break every consumer testing for
/// `"exact"`, and V9 calls that field a contract. The human label carries
/// the qualifier inline instead: different idioms, one rule (V101/V9).
///
/// Absent for the local tiers -- no key rather than an empty one, since
/// there is no endpoint to report, not an unknown one.
fn endpoint(method: &Method) -> String {
    match &method.endpoint {
        Some(e) => format!(",\"endpoint\":\"{}\"", escape(e)),
        None => String::new(),
    }
}

/// Minimal JSON string escaping: quote, backslash, and the control
/// characters a path could carry. Enough for a filesystem path, which is
/// all this field ever holds.
#[must_use]
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

/// The full JSONL report: one object per line.
#[must_use]
pub fn report(ests: &[Estimate], method: &Method) -> String {
    ests.iter()
        .map(|e| format!("{}\n", object(e, method)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::DUMMY;

    fn est(path: &str, tokens: u64) -> Estimate {
        Estimate {
            path: path.to_owned(),
            tokens,
        }
    }

    #[test]
    fn object_carries_the_v3_fields() {
        let o = object(&est("a.rs", 12), &DUMMY);
        assert!(o.contains("\"path\":\"a.rs\""));
        assert!(o.contains("\"tokens\":12"));
        assert!(o.contains("\"unit\":\"input_tokens\""));
        assert!(o.contains("\"estimated\":true"));
        assert!(o.contains("\"method\":\"bytes/4\""));
    }

    #[test]
    fn an_exact_tier_is_not_estimated() {
        let exact = Method {
            tier: "exact",
            endpoint: None,
            approximate: false,
        };
        let o = object(&est("a.rs", 12), &exact);
        assert!(o.contains("\"estimated\":false"));
    }

    #[test]
    fn escape_handles_quotes_and_backslashes() {
        assert_eq!(escape(r#"a"b\c"#), r#"a\"b\\c"#);
    }

    #[test]
    fn escape_handles_control_characters() {
        assert_eq!(escape("a\nb\rc\td"), "a\\nb\\rc\\td");
    }

    #[test]
    fn report_is_one_object_per_line() {
        let out = report(&[est("a", 1), est("b", 2)], &DUMMY);
        assert_eq!(out.lines().count(), 2);
    }
}
