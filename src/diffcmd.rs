//! The `diff` command: token delta between two points, in git diff's
//! arg-forms (V7/V32). Built on the gitref primitive -- each side is
//! either a ref (a blob at a commit) or the working tree:
//!
//! ```text
//! itok diff              working tree vs HEAD
//! itok diff <ref>        working tree vs <ref>
//! itok diff <A> <B>      <B> vs <A>   (also <A>..<B>)
//! itok diff --staged     index vs HEAD
//! ... [-- <path>]        narrow to one path (V33)
//! ```
//!
//! Report-only unless a gate is asked for: --exit-code (nonzero on any
//! delta) or --budget N (nonzero when the delta adds more than N, V16).

use crate::args::Format;
use crate::cli::Output;
use crate::diffargs::{Raw, breach_msg, parse, signed};
use crate::estimate;
use crate::gitref;
use crate::render::{DUMMY, Method, O200K};
use std::path::{Path, PathBuf};

/// One side of a diff.
enum Side {
    Ref(String),
    Working,
}

/// A resolved comparison: old ref, new side, tier. Bundles what every
/// per-file step needs so no function grows past the argument limit.
struct Cmp {
    old: String,
    new: Side,
    bpe: bool,
}

pub(crate) fn diff(rest: &[String]) -> Output {
    match parse(rest) {
        Ok(raw) => run(&raw),
        Err(e) => Output::usage_err(format!("itok: {e}")),
    }
}

fn run(raw: &Raw) -> Output {
    let root = PathBuf::from(raw.chdir.as_deref().unwrap_or("."));
    let (old, new) = sides(raw);
    let files = select(&root, raw, &old, &new);
    let cmp = Cmp {
        old,
        new,
        bpe: raw.bpe,
    };
    let rows: Vec<(String, i64)> = files
        .iter()
        .filter(|f| raw.path.as_ref().is_none_or(|p| *f == p))
        .map(|f| (f.clone(), delta(&root, &cmp, f)))
        .collect();
    verdict(raw, &cmp, &rows)
}

/// The two sides, from the parsed form.
fn sides(raw: &Raw) -> (String, Side) {
    if raw.staged {
        return ("HEAD".to_owned(), Side::Ref(String::new())); // "" = index
    }
    match raw.refs.as_slice() {
        [] => ("HEAD".to_owned(), Side::Working),
        [one] => split_range(one),
        [a, b, ..] => (a.clone(), Side::Ref(b.clone())),
    }
}

/// `A..B` -> (A, B); a bare ref -> (ref, working).
fn split_range(one: &str) -> (String, Side) {
    match one.split_once("..") {
        Some((a, b)) => (a.to_owned(), Side::Ref(b.to_owned())),
        None => (one.to_owned(), Side::Working),
    }
}

/// The files to compare, per side.
fn select(root: &Path, raw: &Raw, old: &str, new: &Side) -> Vec<String> {
    if raw.staged {
        gitref::staged(root)
    } else {
        match new {
            Side::Ref(b) => gitref::changed_between(root, old, b),
            Side::Working => gitref::changed_working(root, old),
        }
    }
}

/// new-side minus old-side tokens for one file (saturating i64).
fn delta(root: &Path, c: &Cmp, path: &str) -> i64 {
    let o = gitref::count_at(root, &c.old, path, c.bpe).unwrap_or(0);
    let n = match &c.new {
        Side::Ref(r) => gitref::count_at(root, r, path, c.bpe).unwrap_or(0),
        Side::Working => estimate::count(&root.join(path), c.bpe).unwrap_or(0),
    };
    let (o, n) = (
        i64::try_from(o).unwrap_or(i64::MAX),
        i64::try_from(n).unwrap_or(i64::MAX),
    );
    n.saturating_sub(o)
}

fn method(bpe: bool) -> &'static Method {
    if bpe { &O200K } else { &DUMMY }
}

/// Total added tokens (positive deltas only): the `--budget` quantity.
fn added(rows: &[(String, i64)]) -> u64 {
    let sum = rows
        .iter()
        .map(|(_, d)| (*d).max(0))
        .fold(0i64, i64::saturating_add);
    u64::try_from(sum).unwrap_or(0)
}

fn total(rows: &[(String, i64)]) -> i64 {
    rows.iter().map(|(_, d)| *d).fold(0i64, i64::saturating_add)
}

fn verdict(raw: &Raw, cmp: &Cmp, rows: &[(String, i64)]) -> Output {
    let out = match raw.format {
        Format::Json => json(raw, rows),
        Format::Human => report(cmp, rows),
    };
    gate(raw, rows, out)
}

/// Apply the requested gate (--budget, then --exit-code) to the report.
fn gate(raw: &Raw, rows: &[(String, i64)], out: String) -> Output {
    if let Some(b) = raw.budget
        && added(rows) > b
    {
        return Output::breach(out, breach_msg(added(rows), b));
    }
    if raw.exit_code && total(rows) != 0 {
        return Output {
            out,
            err: String::new(),
            code: 1,
        };
    }
    Output::ok(out)
}

fn report(c: &Cmp, rows: &[(String, i64)]) -> String {
    let m = method(c.bpe);
    let head = match &c.new {
        Side::Ref(r) if r.is_empty() => format!("{} vs index", c.old),
        Side::Ref(r) => format!("{}..{r}", c.old),
        Side::Working => format!("{}..working", c.old),
    };
    let mut s = format!("{head}  ({})\n", m.label());
    for (p, d) in rows {
        s.push_str(&format!("{:>10} itok  {p}\n", signed(*d)));
    }
    s.push_str(&format!("{:>10} itok  total\n", signed(total(rows))));
    s
}

fn json(raw: &Raw, rows: &[(String, i64)]) -> String {
    let method = method(raw.bpe).label();
    let files: String = rows
        .iter()
        .map(|(p, d)| {
            format!("{{\"path\":\"{}\",\"delta\":{d}}}", crate::json::escape(p))
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"method\":\"{method}\",\"total\":{},\"added\":{},\"files\":[{files}]}}\n",
        total(rows),
        added(rows),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Repo root & crate-relative paths come from git (V37), so these tests
    // pass in-tree AND in the extraction rehearsal (T11).
    fn root() -> String {
        crate::testutil::repo_root()
    }

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn two_refs_report_a_delta_and_total() {
        let o = diff(&args(&["-C", &root(), "HEAD~1", "HEAD"]));
        assert_eq!(o.code, 0);
        assert!(o.out.contains("total"));
        assert!(o.out.contains("itok"));
    }

    #[test]
    fn range_form_is_the_same_as_two_refs() {
        let a = diff(&args(&["-C", &root(), "HEAD~1", "HEAD"])).out;
        let b = diff(&args(&["-C", &root(), "HEAD~1..HEAD"])).out;
        // Same rows; only the header differs (A B vs A..B).
        assert_eq!(a.lines().count(), b.lines().count());
    }

    #[test]
    fn path_filter_narrows_to_one_file() {
        let o = diff(&args(&[
            "-C",
            &root(),
            "HEAD~1",
            "HEAD",
            "--",
            &crate::testutil::crate_path("SPEC.md"),
        ]));
        // At most the one file plus the total line.
        assert!(o.out.lines().count() <= 3);
    }

    #[test]
    fn exit_code_is_one_when_there_is_a_delta() {
        if !crate::testutil::dogfood() {
            return;
        }
        let o = diff(&args(&["-C", &root(), "--exit-code", "HEAD~1", "HEAD"]));
        assert_eq!(o.code, 1);
    }

    #[test]
    fn budget_breaches_when_the_change_adds_too_much() {
        if !crate::testutil::dogfood() {
            return;
        }
        // The empty tree -> HEAD adds the ENTIRE repo, so budget 1 always
        // breaches -- STATE-independent (V37). `HEAD~1..HEAD` couples to
        // the last commit's size: a near-zero-delta commit as HEAD (a
        // config number bump) made this spuriously pass with no breach
        // (B3). The empty-tree hash is git's well-known constant.
        const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
        let o =
            diff(&args(&["-C", &root(), "--budget", "1", EMPTY_TREE, "HEAD"]));
        assert_eq!(o.code, 1);
        assert!(o.err.contains("over budget"));
    }

    #[test]
    fn json_carries_total_and_files() {
        let o = diff(&args(&[
            "-C",
            &root(),
            "--format",
            "json",
            "HEAD~1",
            "HEAD",
        ]));
        assert!(o.out.contains("\"total\":"));
        assert!(o.out.contains("\"files\":["));
    }

    #[test]
    fn a_bad_flag_is_a_usage_error() {
        assert_eq!(diff(&args(&["--bogus"])).code, 2);
    }

    #[test]
    fn no_refs_compares_working_to_head() {
        let o = diff(&args(&["-C", &root()]));
        assert_eq!(o.code, 0);
        assert!(o.out.contains("working"));
    }

    #[test]
    fn a_bare_ref_compares_working_to_it() {
        let o = diff(&args(&["-C", &root(), "HEAD"]));
        assert_eq!(o.code, 0);
        assert!(o.out.contains("working"));
    }

    #[test]
    fn the_staged_form_runs() {
        let o = diff(&args(&["-C", &root(), "--staged"]));
        assert_eq!(o.code, 0);
        assert!(o.out.contains("index"));
    }

    #[test]
    fn delta_reads_the_working_side() {
        // Exercises delta's Working branch regardless of git state.
        let c = Cmp {
            old: "HEAD".to_owned(),
            new: Side::Working,
            bpe: false,
        };
        let _ = delta(Path::new(&root()), &c, "Cargo.toml");
    }
}
