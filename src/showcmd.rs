//! The `show` command: one commit's per-file token delta (V32), matching
//! git show. Default HEAD; `<commit>:<path>` shows a single blob's cost
//! at a ref (git's object syntax); `-- <path>` narrows the breakdown.
//! Report-only. Built on the gitref primitive.

use crate::args::Format;
use crate::cli::Output;
use crate::diffargs::signed;
use crate::gitref;
use crate::render::{DUMMY, Method, O200K};
use std::path::{Path, PathBuf};

#[derive(Default)]
struct Raw {
    commit: Option<String>,
    path: Option<String>,
    bpe: bool,
    format: Format,
    chdir: Option<String>,
}

pub(crate) fn show(rest: &[String]) -> Output {
    match parse(rest) {
        Ok(raw) => run(&raw),
        Err(e) => Output::usage_err(format!("itok: {e}")),
    }
}

fn run(raw: &Raw) -> Output {
    let root = PathBuf::from(raw.chdir.as_deref().unwrap_or("."));
    let commit = raw.commit.as_deref().unwrap_or("HEAD");
    if let Some((c, p)) = commit.split_once(':') {
        return Output::ok(blob_cost(&root, c, p, raw.bpe));
    }
    let rows = rows(&root, commit, raw);
    Output::ok(match raw.format {
        Format::Json => json(commit, raw.bpe, &rows),
        Format::Human => report(&root, commit, raw.bpe, &rows),
    })
}

fn method(bpe: bool) -> &'static Method {
    if bpe { &O200K } else { &DUMMY }
}

/// A single blob's cost at a ref (`<commit>:<path>`).
fn blob_cost(root: &Path, commit: &str, path: &str, bpe: bool) -> String {
    let n = gitref::count_at(root, commit, path, bpe).unwrap_or(0);
    format!("{n} itok  {path}@{commit} ({})\n", method(bpe).label())
}

/// Per-file (path, delta) for the files the commit changed, path-filtered.
fn rows(root: &Path, commit: &str, raw: &Raw) -> Vec<(String, i64)> {
    let parent = format!("{commit}~1");
    gitref::changed_in(root, commit)
        .into_iter()
        .filter(|f| raw.path.as_ref().is_none_or(|p| p == f))
        .map(|f| {
            let d = file_delta(root, (&parent, commit), &f, raw.bpe);
            (f, d)
        })
        .collect()
}

/// new-ref minus old-ref tokens for one file (saturating i64).
fn file_delta(root: &Path, refs: (&str, &str), path: &str, bpe: bool) -> i64 {
    let (old, new) = refs;
    let o = gitref::count_at(root, old, path, bpe).unwrap_or(0);
    let n = gitref::count_at(root, new, path, bpe).unwrap_or(0);
    i64::try_from(n)
        .unwrap_or(i64::MAX)
        .saturating_sub(i64::try_from(o).unwrap_or(i64::MAX))
}

fn total(rows: &[(String, i64)]) -> i64 {
    rows.iter().map(|(_, d)| *d).fold(0i64, i64::saturating_add)
}

fn report(
    root: &Path,
    commit: &str,
    bpe: bool,
    rows: &[(String, i64)],
) -> String {
    let subj =
        gitref::subject(root, commit).unwrap_or_else(|| commit.to_owned());
    let mut s = format!("{subj}  ({})\n", method(bpe).label());
    for (p, d) in rows {
        s.push_str(&format!("{:>10} itok  {p}\n", signed(*d)));
    }
    s.push_str(&format!("{:>10} itok  total\n", signed(total(rows))));
    s
}

fn json(commit: &str, bpe: bool, rows: &[(String, i64)]) -> String {
    let files: String = rows
        .iter()
        .map(|(p, d)| {
            format!("{{\"path\":\"{}\",\"delta\":{d}}}", crate::json::escape(p))
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"commit\":\"{}\",\"method\":\"{}\",\"total\":{},\"files\":[{files}]}}\n",
        crate::json::escape(commit),
        method(bpe).label(),
        total(rows),
    )
}

fn parse(rest: &[String]) -> Result<Raw, String> {
    let mut r = Raw::default();
    let mut i = 0usize;
    let mut past = false;
    while let Some(a) = rest.get(i) {
        if past {
            r.path = Some(a.clone());
        } else if a == "--" {
            past = true;
        } else {
            flag(&mut r, a, rest, &mut i)?;
        }
        i = i.saturating_add(1);
    }
    Ok(r)
}

fn flag(
    r: &mut Raw,
    a: &str,
    rest: &[String],
    i: &mut usize,
) -> Result<(), String> {
    match a {
        #[cfg(feature = "bpe")]
        "--bpe" => r.bpe = true,
        "--format" => r.format = fmt(&val(rest, i)?)?,
        "-C" => r.chdir = Some(val(rest, i)?),
        p if p.starts_with('-') => return Err(format!("unknown flag '{p}'")),
        p => r.commit = Some(p.to_owned()),
    }
    Ok(())
}

fn val(rest: &[String], i: &mut usize) -> Result<String, String> {
    *i = i.saturating_add(1);
    rest.get(*i)
        .cloned()
        .ok_or_else(|| "flag needs a value".to_owned())
}

fn fmt(s: &str) -> Result<Format, String> {
    match s {
        "json" => Ok(Format::Json),
        "human" => Ok(Format::Human),
        other => Err(format!("unknown format '{other}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Repo root & crate-relative paths from git (V37): in-tree and the
    // extraction rehearsal (T11) both pass.
    fn root() -> String {
        crate::testutil::repo_root()
    }

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn a_commit_shows_subject_and_total() {
        let o = show(&args(&["-C", &root(), "HEAD"]));
        assert_eq!(o.code, 0);
        assert!(o.out.contains("total"));
        assert!(o.out.contains("itok"));
    }

    #[test]
    fn default_is_head() {
        let a = show(&args(&["-C", &root()])).out;
        let b = show(&args(&["-C", &root(), "HEAD"])).out;
        assert_eq!(a.lines().count(), b.lines().count());
    }

    #[test]
    fn blob_form_shows_a_single_cost() {
        let o = show(&args(&["-C", &root(), "HEAD:Cargo.toml"]));
        assert_eq!(o.code, 0);
        assert!(o.out.contains("Cargo.toml@HEAD"));
        assert!(!o.out.contains("total"), "blob is one number, not a diff");
    }

    #[test]
    fn path_filter_narrows_the_breakdown() {
        let o = show(&args(&[
            "-C",
            &root(),
            "HEAD",
            "--",
            &crate::testutil::crate_path("SPEC.md"),
        ]));
        assert!(o.out.lines().count() <= 3);
    }

    #[test]
    fn json_carries_commit_and_files() {
        let o = show(&args(&["-C", &root(), "--format", "json", "HEAD"]));
        assert!(o.out.contains("\"commit\":"));
        assert!(o.out.contains("\"files\":["));
    }

    #[test]
    fn a_bad_flag_is_a_usage_error() {
        assert_eq!(show(&args(&["--bogus"])).code, 2);
    }

    #[test]
    fn a_bad_format_is_a_usage_error() {
        assert_eq!(show(&args(&["--format", "yaml"])).code, 2);
    }

    #[cfg(feature = "bpe")]
    #[test]
    fn bpe_labels_the_o200k_method() {
        let o = show(&args(&["-C", &root(), "--bpe", "HEAD"]));
        assert!(o.out.contains("o200k"));
    }
}
