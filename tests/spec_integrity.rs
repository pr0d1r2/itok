//! `SPEC.md` is the project's memory, and nothing standalone checked it:
//! `cavekit-spec` is a host tool that does not travel (V31), so in this
//! repo the spec could rot silently. Compaction (T49) made that concrete
//! -- an edit pass that drops an invariant or breaks a citation is
//! exactly the failure a byte-count cannot see.
//!
//! These are STRUCTURAL checks. They cannot prove a rewrite preserved
//! meaning, but they catch the dangerous class: a vanished invariant, a
//! citation pointing at nothing, a section that lost its header.

use std::path::Path;

fn spec() -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("SPEC.md");
    let text = std::fs::read_to_string(p).unwrap_or_default();
    assert!(!text.is_empty(), "SPEC.md is missing or empty");
    text
}

/// Ids declared at the start of a line, e.g. `V42:` or `T30b|`.
fn declared(text: &str, prefix: char, terminator: char) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let rest = l.strip_prefix(prefix)?;
            let idx = rest.find(terminator)?;
            let id = rest.get(..idx)?;
            let ok = !id.is_empty()
                && id.chars().next().is_some_and(|c| c.is_ascii_digit())
                && id.chars().all(|c| c.is_ascii_alphanumeric());
            ok.then(|| format!("{prefix}{id}"))
        })
        .collect()
}

/// Every `V<n>` mentioned anywhere -- citations from any section.
fn cited(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in text.split(|c: char| !c.is_ascii_alphanumeric()) {
        let Some(num) = token.strip_prefix('V') else {
            continue;
        };
        if !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()) {
            out.push(token.to_owned());
        }
    }
    out
}

/// FORMAT.md fixes the sections and their order.
/// `\u{a7}` is the section sign. Written as an escape because the source
/// must stay ASCII (Trojan-Source; `tests/hygiene.rs` enforces it) -- the
/// runtime string is identical either way.
const SECTIONS: [&str; 6] = [
    "## \u{a7}G GOAL",
    "## \u{a7}C CONSTRAINTS",
    "## \u{a7}I INTERFACE",
    "## \u{a7}V INVARIANTS",
    "## \u{a7}T TASKS",
    "## \u{a7}B BUGS",
];

#[test]
fn sections_are_present_and_ordered() {
    let text = spec();
    let mut last = 0usize;
    for header in SECTIONS {
        let at = text.find(header);
        assert!(at.is_some(), "SPEC.md lost its `{header}` section");
        let at = at.unwrap_or(0);
        assert!(at > last, "`{header}` is out of order");
        last = at;
    }
}

/// Invariant numbers are UNIQUE and never reused (FORMAT.md). A gap is
/// permitted -- V63 is one, a number skipped while drafting -- because a
/// skipped number costs nothing, while a REUSED number silently
/// redirects every citation that pointed at the old meaning.
#[test]
fn invariant_numbers_are_unique() {
    let ids = declared(&spec(), 'V', ':');
    assert!(!ids.is_empty(), "no invariants found -- parser broken?");
    let mut seen = ids.clone();
    seen.sort();
    seen.dedup();
    assert_eq!(
        seen.len(),
        ids.len(),
        "an invariant number is declared twice -- numbers are never reused, \
         because a citation to the old meaning would silently redirect"
    );
}

/// Every citation resolves. A dangling `V99` is a pointer into nothing,
/// and it reads as authoritative -- the V81 failure at spec level.
#[test]
fn every_citation_resolves() {
    let text = spec();
    let declared_ids = declared(&text, 'V', ':');
    for c in cited(&text) {
        assert!(
            declared_ids.contains(&c),
            "`{c}` is cited but never declared"
        );
    }
}

/// Task and bug ids are unique. A duplicate means two different pieces of
/// work claim one identity, so a citation to it is ambiguous.
#[test]
fn task_and_bug_ids_are_unique() {
    let text = spec();
    for (prefix, label) in [('T', "task"), ('B', "bug")] {
        let ids = declared(&text, prefix, '|');
        let mut seen = ids.clone();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), ids.len(), "duplicate {label} id in SPEC.md");
    }
}

/// The audit trail V83 depends on: options considered and rejected, each
/// with the trigger that would reopen it. Compaction must never trade
/// these away for bytes -- they are what makes a decision auditable
/// rather than merely obeyable.
///
/// Named invariants rather than a word count: a threshold on how often a
/// word appears is arbitrary, and an arbitrary threshold either fires on
/// nothing or fires on prose edits that changed no decision.
const AUDIT_RECORDS: [(&str, &str); 5] = [
    ("V21", "DEFERRED"),
    ("V24", "rejected"),
    ("V25", "rejected"),
    ("V35", "REJECTED"),
    ("V58", "DEFERRED"),
];

/// The body of one invariant, up to the next one.
fn invariant_body(text: &str, id: &str) -> String {
    text.split_once(&format!("\n{id}:"))
        .and_then(|(_, rest)| rest.split_once("\nV").map(|(b, _)| b.to_owned()))
        .unwrap_or_default()
}

#[test]
fn the_rejected_option_records_survive() {
    let text = spec();
    for (id, marker) in AUDIT_RECORDS {
        assert!(
            invariant_body(&text, id).contains(marker),
            "{id} lost its `{marker}` record -- that is the audit trail (V83)"
        );
    }
}
