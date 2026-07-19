//! The `log` command: a path's token cost across history (V19), matching
//! git log. One line per commit that touched the path -- hash, date,
//! subject, the cost AT that commit, and the delta it introduced (cost
//! minus the parent's), like `git log --stat`'s +/-. Report-only. Range
//! (`A..B`), `-n`, `--since`, `--reverse`, and `-- path` are git's own
//! (V32/V33). Built on the gitref primitive.

use crate::args::Format;
use crate::cli::Output;
use crate::diffargs::signed;
use crate::gitref;
use crate::render::{Method, DUMMY, O200K};
use std::path::{Path, PathBuf};

type Row = (String, String, String, u64, i64);

#[derive(Default)]
struct Raw {
    path: Option<String>,
    range: Option<String>,
    limit: Option<String>,
    since: Option<String>,
    reverse: bool,
    bpe: bool,
    format: Format,
    chdir: Option<String>,
}

pub(crate) fn log(rest: &[String]) -> Output {
    match parse(rest) {
        Ok(raw) => run(&raw),
        Err(e) => Output::usage_err(format!("itok: {e}")),
    }
}

fn run(raw: &Raw) -> Output {
    let root = PathBuf::from(raw.chdir.as_deref().unwrap_or("."));
    let Some(path) = raw.path.clone() else {
        return Output::usage_err("itok: log needs a path".to_owned());
    };
    let commits = gitref::commits(&root, &git_args(raw, &path));
    let rows = rows(&root, &commits, &path, raw.bpe);
    Output::ok(match raw.format {
        Format::Json => json(&path, raw.bpe, &rows),
        Format::Human => report(&path, raw.bpe, &rows),
    })
}

fn method(bpe: bool) -> &'static Method {
    if bpe {
        &O200K
    } else {
        &DUMMY
    }
}

/// The git-log selectors, in git's own order, ending with `-- <path>`.
fn git_args(raw: &Raw, path: &str) -> Vec<String> {
    let mut a: Vec<String> = raw.range.clone().into_iter().collect();
    if let Some(n) = &raw.limit {
        a.push("-n".to_owned());
        a.push(n.clone());
    }
    a.extend(raw.since.as_ref().map(|s| format!("--since={s}")));
    if raw.reverse {
        a.push("--reverse".to_owned());
    }
    a.push("--".to_owned());
    a.push(path.to_owned());
    a
}

fn rows(
    root: &Path,
    commits: &[(String, String, String)],
    path: &str,
    bpe: bool,
) -> Vec<Row> {
    commits
        .iter()
        .map(|(h, d, s)| {
            let cost = gitref::count_at(root, h, path, bpe).unwrap_or(0);
            let parent = format!("{h}~1");
            let was = gitref::count_at(root, &parent, path, bpe).unwrap_or(0);
            (h.clone(), d.clone(), s.clone(), cost, delta(cost, was))
        })
        .collect()
}

fn delta(cost: u64, was: u64) -> i64 {
    i64::try_from(cost)
        .unwrap_or(i64::MAX)
        .saturating_sub(i64::try_from(was).unwrap_or(i64::MAX))
}

fn report(path: &str, bpe: bool, rows: &[Row]) -> String {
    let mut s = format!("{path}  ({})\n", method(bpe).label);
    for (h, date, subj, cost, d) in rows {
        s.push_str(&format!(
            "{h}  {date}  {cost:>8} itok  {:>7}  {subj}\n",
            signed(*d)
        ));
    }
    s
}

fn json(path: &str, bpe: bool, rows: &[Row]) -> String {
    let items: String = rows
        .iter()
        .map(|(h, date, subj, cost, d)| {
            format!(
                "{{\"commit\":\"{h}\",\"date\":\"{date}\",\"subject\":\"{}\",\"tokens\":{cost},\"delta\":{d}}}",
                crate::json::escape(subj)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"path\":\"{}\",\"method\":\"{}\",\"commits\":[{items}]}}\n",
        crate::json::escape(path),
        method(bpe).label,
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
        "--reverse" => r.reverse = true,
        #[cfg(feature = "bpe")]
        "--bpe" => r.bpe = true,
        "-n" => r.limit = Some(val(rest, i)?),
        "--since" => r.since = Some(val(rest, i)?),
        "--format" => r.format = fmt(&val(rest, i)?)?,
        "-C" => r.chdir = Some(val(rest, i)?),
        p if p.starts_with('-') => return Err(format!("unknown flag '{p}'")),
        p if p.contains("..") => r.range = Some(p.to_owned()),
        p => r.path = Some(p.to_owned()),
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

    fn path(rel: &str) -> String {
        crate::testutil::crate_path(rel)
    }

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn a_paths_history_has_costs() {
        let o = log(&args(&["-C", &root(), &path("Cargo.toml")]));
        assert_eq!(o.code, 0);
        assert!(o.out.contains("itok"));
    }

    #[test]
    fn no_path_is_a_usage_error() {
        let o = log(&args(&["-C", &root()]));
        assert_eq!(o.code, 2);
        assert!(o.err.contains("needs a path"));
    }

    #[test]
    fn n_limits_the_commits() {
        let o = log(&args(&["-C", &root(), "-n", "1", &path("Cargo.toml")]));
        // header + at most one commit line.
        assert!(o.out.lines().count() <= 2);
    }

    #[test]
    fn reverse_and_range_are_accepted() {
        let o = log(&args(&[
            "-C",
            &root(),
            "--reverse",
            "HEAD~3..HEAD",
            "--",
            &path("SPEC.md"),
        ]));
        assert_eq!(o.code, 0);
    }

    #[test]
    fn json_carries_the_commits() {
        let o = log(&args(&[
            "-C",
            &root(),
            "--format",
            "json",
            &path("Cargo.toml"),
        ]));
        assert!(o.out.contains("\"commits\":["));
        assert!(o.out.contains("\"delta\":"));
    }

    #[test]
    fn a_bad_flag_is_a_usage_error() {
        assert_eq!(log(&args(&["--bogus"])).code, 2);
    }

    #[test]
    fn a_flag_missing_its_value_errors() {
        assert_eq!(log(&args(&["-n"])).code, 2);
    }

    #[cfg(feature = "bpe")]
    #[test]
    fn bpe_labels_o200k() {
        let o = log(&args(&["-C", &root(), "--bpe", &path("Cargo.toml")]));
        assert!(o.out.contains("o200k"));
    }
}
