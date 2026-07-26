//! Resolving a requested `--model` against what a fleet actually serves
//! (V6, on a DISCOVERED set instead of the verb table).
//!
//! Lives here rather than in a verb because BOTH verbs that take
//! `--ollama` need it: `doctor` picks a model to report fit against, and
//! `estimate` picks the tokenizer to count with. B11a is what a second
//! copy costs -- the resolution landed in `discover` only, so
//! `estimate --ollama --model gpt-oss` 404'd while `doctor` resolved the
//! same token. One definition, many callers (V64).
//!
//! Order: exact, then ollama's OWN `name` = `name:latest`, then a unique
//! prefix. Step two matters -- `ollama run codestral` means
//! `codestral:latest`, so meaning anything else by the same token would
//! invoke that prior and then violate it (V2's expensive failure). The
//! prefix rung extends past ollama's grammar rather than bending it.
//!
//! Never silent-picks: several hits is `Ambiguous`, and the message names
//! the candidates.

/// How a requested `--model` resolved against what the fleet serves.
pub(crate) enum Resolved<'a> {
    One(&'a str),
    Ambiguous(Vec<&'a str>),
    Missing,
}

pub(crate) fn resolve<'a>(names: &[&'a str], want: &str) -> Resolved<'a> {
    if let Some(n) = named(names, want) {
        return Resolved::One(n);
    }
    let hits: Vec<&'a str> = names
        .iter()
        .copied()
        .filter(|n| n.starts_with(want))
        .collect();
    match hits.as_slice() {
        [one] => Resolved::One(one),
        [] => Resolved::Missing,
        _ => Resolved::Ambiguous(hits),
    }
}

/// The two rungs that are NOT inference: a fully spelled name, then
/// ollama's own `name` = `name:latest`. Checked in that order so a fleet
/// carrying both `foo` and `foo:latest` resolves `foo` to itself rather
/// than to whichever the list happened to hold first.
fn named<'a>(names: &[&'a str], want: &str) -> Option<&'a str> {
    let latest = format!("{want}:latest");
    names
        .iter()
        .copied()
        .find(|n| *n == want)
        .or_else(|| names.iter().copied().find(|n| *n == latest))
}

pub(crate) fn ambiguous_msg(want: &str, candidates: &[&str]) -> String {
    format!("'{want}' is ambiguous: {}", candidates.join(", "))
}

/// Names what IS served, and how many hosts answered.
///
/// The host count is not decoration: a named host that did not answer is
/// skipped (V24's resilience), so this list can be INCOMPLETE and the
/// absent model might live on the host that was down. Saying `2/3
/// host(s)` is the difference between "not served" and "not seen" --
/// B11b, and V44's rule that a partial answer must not read as whole.
pub(crate) fn missing_msg(
    want: &str,
    served: &[&str],
    answered: usize,
    named_hosts: usize,
) -> String {
    format!(
        "no fleet host serves '{want}' -- served by {answered}/{named_hosts} \
         host(s): {}",
        served.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLEET: [&str; 4] = [
        "codestral:latest",
        "gpt-oss:20b",
        "qwen3-coder:30b",
        "qwen3-coder:7b",
    ];

    fn one(want: &str) -> Option<&'static str> {
        match resolve(&FLEET, want) {
            Resolved::One(m) => Some(m),
            _ => None,
        }
    }

    /// The ask: a family name narrows to the one model serving it.
    #[test]
    fn a_unique_prefix_resolves() {
        assert_eq!(one("gpt-oss"), Some("gpt-oss:20b"));
        assert_eq!(one("gpt"), Some("gpt-oss:20b"));
    }

    /// An exact name wins outright -- the prefix rung never gets to
    /// second-guess a fully spelled model.
    #[test]
    fn an_exact_name_wins() {
        assert_eq!(one("gpt-oss:20b"), Some("gpt-oss:20b"));
        assert_eq!(one("qwen3-coder:7b"), Some("qwen3-coder:7b"));
    }

    /// V2: `ollama run codestral` means `codestral:latest`, so the same
    /// token must mean that here.
    #[test]
    fn a_bare_name_means_latest_as_ollama_means_it() {
        assert_eq!(one("codestral"), Some("codestral:latest"));
    }

    /// Where `:latest` and the prefix rung DISAGREE, ollama's grammar
    /// wins. This is the case that proves the rule; the ones where both
    /// rungs agree prove nothing.
    #[test]
    fn latest_beats_an_otherwise_ambiguous_prefix() {
        let fleet = ["qwen3-coder:latest", "qwen3-coder:30b", "qwen3-coder:7b"];
        let got = match resolve(&fleet, "qwen3-coder") {
            Resolved::One(m) => Some(m),
            _ => None,
        };
        assert_eq!(got, Some("qwen3-coder:latest"));
    }

    /// V6: several hits ERROR with the candidates, never a silent pick.
    #[test]
    fn an_ambiguous_prefix_names_its_candidates() {
        let got = match resolve(&FLEET, "qwen") {
            Resolved::Ambiguous(c) => c,
            _ => Vec::new(),
        };
        assert_eq!(got, vec!["qwen3-coder:30b", "qwen3-coder:7b"]);
    }

    #[test]
    fn an_unserved_name_is_missing() {
        assert!(matches!(resolve(&FLEET, "llama"), Resolved::Missing));
        assert!(matches!(resolve(&[], "gpt-oss"), Resolved::Missing));
    }

    /// B11b/V44: the not-found message says how many hosts answered, so
    /// "not served" stays distinguishable from "not seen".
    #[test]
    fn the_missing_message_reports_host_coverage() {
        let m = missing_msg("llama", &["a", "b"], 2, 3);
        assert!(m.contains("2/3 host(s)"), "coverage stated: {m}");
        assert!(m.contains("a, b"));
    }

    #[test]
    fn the_ambiguous_message_lists_candidates() {
        assert!(ambiguous_msg("q", &["q:1", "q:2"]).contains("q:1, q:2"));
    }
}
