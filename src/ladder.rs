//! The reduction ladder (V50): ordered LOSSLESS -> LOSSIEST, applied in
//! order, stopping the moment the budget is met.
//!
//! `cap` is the last rung and the one that was always here -- a hard
//! truncate. The four above it try to fit the budget without dropping the
//! tail, and the footer names every one that ran, because a reader who
//! cannot tell which rung cut their text cannot judge what is missing.
//!
//! ORDER IS THE INVARIANT, not the flag order. `--outline --strip` runs
//! strip first, because the ladder is about how much a rung costs the
//! reader, and letting the caller reorder it would put structural loss
//! ahead of whitespace removal -- the exact failure V50 exists to prevent.
//!
//! Every rung is a pure function of the text and deterministic, so the
//! same input cut with the same rungs yields the same output (V51).

/// One rung, cheapest first. The order of this enum IS the ladder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Rung {
    /// ANSI escapes and trailing whitespace. Bytes that carry no meaning
    /// to a reader of the text.
    Strip,
    /// Runs of identical consecutive lines collapse to one, marked `xN`.
    /// The count is KEPT: how many times is information, and a dedup that
    /// dropped it would be lossy while claiming not to be.
    Dedup,
    /// The bodies of base64 and minified lines, replaced by a marker
    /// naming their size. LOSSY.
    Elide,
    /// Indented bodies dropped, signature-shaped lines kept. LOSSY, and a
    /// heuristic: it reads indentation, not syntax.
    Outline,
}

impl Rung {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Strip => "strip",
            Self::Dedup => "dedup",
            Self::Elide => "elide",
            Self::Outline => "outline",
        }
    }

    pub(crate) fn parse(flag: &str) -> Option<Self> {
        match flag {
            "--strip" => Some(Self::Strip),
            "--dedup" => Some(Self::Dedup),
            "--elide" => Some(Self::Elide),
            "--outline" => Some(Self::Outline),
            _ => None,
        }
    }

    /// Apply this rung to the whole text.
    pub(crate) fn apply(self, text: &str) -> String {
        match self {
            Self::Strip => strip(text),
            Self::Dedup => dedup(text),
            Self::Elide => elide(text),
            Self::Outline => outline(text),
        }
    }
}

/// ANSI escape sequences and trailing whitespace, gone.
///
/// CSI and OSC both, because a terminal recording carries both and half a
/// stripper leaves the half that is hardest to read.
fn strip(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let ends = line.ends_with('\n');
        let bare = strip_ansi(line.trim_end_matches('\n'));
        out.push_str(bare.trim_end_matches([' ', '\t']));
        if ends {
            out.push('\n');
        }
    }
    out
}

fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        // CSI ends at a letter, OSC at BEL or ESC-backslash. Anything
        // else after ESC is a two-character sequence.
        for next in chars.by_ref() {
            if next.is_ascii_alphabetic() || next == '\u{7}' {
                break;
            }
        }
    }
    out
}

/// Consecutive identical lines collapse, and the COUNT is kept.
fn dedup(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut runs: Vec<(&str, u64)> = Vec::new();
    for line in text.split_inclusive('\n') {
        match runs.last_mut() {
            Some((prev, n)) if *prev == line => *n = n.saturating_add(1),
            _ => runs.push((line, 1)),
        }
    }
    for (line, n) in runs {
        out.push_str(&run_line(line, n));
    }
    out
}

fn run_line(line: &str, n: u64) -> String {
    if n < 2 {
        return line.to_owned();
    }
    let body = line.trim_end_matches('\n');
    let tail = if line.ends_with('\n') { "\n" } else { "" };
    format!("{body}  x{n}{tail}")
}

/// The threshold a line must pass to be considered a blob. Short base64
/// is a checksum someone wanted to read; a 200-character run of it is a
/// payload nobody does.
const BLOB: usize = 200;

/// Base64 and minified bodies, replaced by their size.
///
/// Conservative by design: a line has to be BOTH long and dense in the
/// base64 alphabet before its content is dropped, because eliding prose
/// that merely looked like data is a loss the reader cannot detect.
fn elide(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches('\n');
        let tail = if line.ends_with('\n') { "\n" } else { "" };
        if blob(body) {
            let n = body.len();
            out.push_str(&format!("<elided: {n} bytes of data>{tail}"));
        } else {
            out.push_str(line);
        }
    }
    out
}

fn blob(body: &str) -> bool {
    if body.len() < BLOB {
        return false;
    }
    let dense = body
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || "+/=_-".contains(*c))
        .count();
    dense.saturating_mul(10) >= body.len().saturating_mul(9)
}

/// Indented bodies dropped, signature-shaped lines kept.
///
/// It reads INDENTATION, not syntax: a language-aware outline would need
/// a parser per language, and a wrong parse silently drops the wrong half.
/// Indentation is a heuristic the footer names as lossy, and the dropped
/// run reports its own line count so the reader knows what is missing.
fn outline(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut dropped = 0u64;
    for line in text.split_inclusive('\n') {
        if keeps(line) {
            out.push_str(&flush(dropped));
            dropped = 0;
            out.push_str(line);
        } else {
            dropped = dropped.saturating_add(1);
        }
    }
    out.push_str(&flush(dropped));
    out
}

fn flush(dropped: u64) -> String {
    if dropped == 0 {
        return String::new();
    }
    format!("... {dropped} line(s) elided\n")
}

/// A line worth keeping: unindented, or blank. Everything deeper is body.
fn keeps(line: &str) -> bool {
    let body = line.trim_end_matches('\n');
    body.trim().is_empty() || !body.starts_with([' ', '\t'])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_enum_order_is_the_ladder_lossless_first() {
        let mut sorted = [Rung::Outline, Rung::Strip, Rung::Elide, Rung::Dedup];
        sorted.sort_unstable();
        assert_eq!(
            sorted,
            [Rung::Strip, Rung::Dedup, Rung::Elide, Rung::Outline]
        );
    }

    #[test]
    fn every_rung_parses_from_its_flag_and_names_itself() {
        for rung in [Rung::Strip, Rung::Dedup, Rung::Elide, Rung::Outline] {
            let flag = format!("--{}", rung.label());
            assert_eq!(Rung::parse(&flag), Some(rung));
        }
        assert_eq!(Rung::parse("--nonesuch"), None);
    }

    #[test]
    fn strip_removes_escapes_and_trailing_space() {
        let got = strip("\u{1b}[32mgreen\u{1b}[0m   \nplain\t\n");
        assert_eq!(got, "green\nplain\n");
    }

    #[test]
    fn strip_keeps_a_final_line_without_a_newline() {
        assert_eq!(strip("tail   "), "tail");
    }

    /// The COUNT is information. A dedup that dropped it would be lossy
    /// while sitting on the lossless half of the ladder.
    #[test]
    fn dedup_collapses_runs_and_keeps_the_count() {
        assert_eq!(dedup("a\na\na\nb\n"), "a  x3\nb\n");
    }

    /// Consecutive only: a global dedup would reorder meaning and break
    /// the promise that the remainder is contiguous from one offset (V51).
    #[test]
    fn dedup_only_collapses_neighbours() {
        assert_eq!(dedup("a\nb\na\n"), "a\nb\na\n");
    }

    #[test]
    fn elide_replaces_a_long_dense_line_with_its_size() {
        let blob = "A".repeat(400);
        let got = elide(&format!("keep\n{blob}\n"));
        assert!(got.starts_with("keep\n"), "{got}");
        assert!(got.contains("<elided: 400 bytes of data>"), "{got}");
    }

    /// Conservative on purpose: eliding prose that merely looked like data
    /// is a loss the reader cannot detect.
    #[test]
    fn elide_leaves_long_prose_alone() {
        let prose = "the quick brown fox jumps over the lazy dog. ".repeat(9);
        assert_eq!(elide(&prose), prose);
    }

    #[test]
    fn elide_leaves_short_data_alone() {
        let short = format!("{}\n", "A".repeat(BLOB - 1));
        assert_eq!(elide(&short), short);
    }

    #[test]
    fn outline_keeps_signatures_and_counts_what_it_dropped() {
        let code = "fn a() {\n    let x = 1;\n    x\n}\nfn b() {\n";
        let got = outline(code);
        assert!(got.contains("fn a() {"), "{got}");
        assert!(got.contains("... 2 line(s) elided"), "{got}");
        assert!(got.contains("fn b() {"), "{got}");
        assert!(!got.contains("let x"), "{got}");
    }

    #[test]
    fn outline_keeps_blank_lines_as_structure() {
        assert_eq!(outline("a\n\nb\n"), "a\n\nb\n");
    }

    /// V51: every rung is a pure function, so a second application of the
    /// same rung to its own output changes nothing further.
    #[test]
    fn every_rung_is_idempotent() {
        let text = "\u{1b}[31mx\u{1b}[0m  \nx\nx\n    body\nfn f() {\n";
        for rung in [Rung::Strip, Rung::Dedup, Rung::Elide, Rung::Outline] {
            let once = rung.apply(text);
            assert_eq!(
                rung.apply(&once),
                once,
                "{} is not idempotent",
                rung.label()
            );
        }
    }
}
