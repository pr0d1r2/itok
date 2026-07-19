//! gitref: the token cost of a file AT a git commit -- the shared
//! primitive under diff/show/log (V33). Each of those reads blobs at refs
//! and counts them, so the git plumbing lives here once: blob content at
//! a ref, the files a commit or a range touched, and a commit's subject.

use std::path::Path;

/// A file's content at `commit` (`git cat-file -p commit:path`), or None
/// if the path did not exist there or git errored.
pub(crate) fn blob(root: &Path, commit: &str, path: &str) -> Option<String> {
    let spec = format!("{commit}:{path}");
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["cat-file", "-p", &spec])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Token count of a file at a commit on the selected tier, or None if the
/// path is absent at that commit.
pub(crate) fn count_at(
    root: &Path,
    commit: &str,
    path: &str,
    bpe: bool,
) -> Option<u64> {
    blob(root, commit, path).map(|text| count_text(&text, bpe))
}

fn count_text(text: &str, bpe: bool) -> u64 {
    #[cfg(feature = "bpe")]
    if bpe {
        return crate::bpe::count(text);
    }
    let _ = bpe;
    crate::estimate::dummy(u64::try_from(text.len()).unwrap_or(u64::MAX))
}

/// Files a single commit changed (`git diff-tree`).
pub(crate) fn changed_in(root: &Path, commit: &str) -> Vec<String> {
    names(
        root,
        &["diff-tree", "--no-commit-id", "--name-only", "-r", commit],
    )
}

/// Files that differ between two refs (`git diff --name-only`).
pub(crate) fn changed_between(root: &Path, a: &str, b: &str) -> Vec<String> {
    names(root, &["diff", "--name-only", a, b])
}

/// Files that differ in the working tree vs a ref.
pub(crate) fn changed_working(root: &Path, refspec: &str) -> Vec<String> {
    names(root, &["diff", "--name-only", refspec])
}

/// Staged files (`git diff --cached --name-only`).
pub(crate) fn staged(root: &Path) -> Vec<String> {
    names(root, &["diff", "--cached", "--name-only"])
}

fn names(root: &Path, args: &[&str]) -> Vec<String> {
    let Ok(out) = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Commits as (short-hash, short-date, subject) for a `git log` with the
/// given extra args (a range, `-n`, `--since`, `--reverse`, `-- path`).
pub(crate) fn commits(
    root: &Path,
    extra: &[String],
) -> Vec<(String, String, String)> {
    let mut args = vec![
        "log".to_owned(),
        "--format=%h%x1f%ad%x1f%s".to_owned(),
        "--date=short".to_owned(),
    ];
    args.extend(extra.iter().cloned());
    output(root, &args).lines().filter_map(triple).collect()
}

/// git stdout for `args`, or empty on error.
fn output(root: &Path, args: &[String]) -> String {
    std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// Split a `%h\x1f%ad\x1f%s` log line into its three fields.
fn triple(line: &str) -> Option<(String, String, String)> {
    let mut p = line.split('\u{1f}');
    Some((
        p.next()?.to_owned(),
        p.next()?.to_owned(),
        p.next()?.to_owned(),
    ))
}

/// A commit's short hash and subject line, for a show/log header.
pub(crate) fn subject(root: &Path, commit: &str) -> Option<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["show", "-s", "--format=%h %s", commit])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // Repo root from git (V37): the monorepo root in-tree, the crate dir in
    // the extraction rehearsal (T11) -- one test, both layouts.
    fn repo() -> PathBuf {
        PathBuf::from(crate::testutil::repo_root())
    }

    #[test]
    fn blob_reads_a_file_at_head() {
        // itok's OWN Cargo.toml, by its repo-relative path -- it carries
        // `[package]` (the workspace manifest, absent once extracted, would
        // carry `[workspace]`). Name-independent so it survives a rename &
        // the read-at-HEAD timing.
        let f = crate::testutil::crate_path("Cargo.toml");
        let b = blob(&repo(), "HEAD", &f).unwrap_or_default();
        assert!(b.contains("[package]"), "got: {b:.40}");
    }

    #[test]
    fn a_missing_path_at_head_is_none() {
        assert_eq!(blob(&repo(), "HEAD", "no/such/file.xyz"), None);
    }

    #[test]
    fn count_at_head_is_positive() {
        assert!(
            count_at(&repo(), "HEAD", "Cargo.toml", false).unwrap_or(0) > 0
        );
    }

    #[test]
    fn count_of_a_missing_path_is_none() {
        assert_eq!(count_at(&repo(), "HEAD", "no/such.xyz", false), None);
    }

    #[test]
    fn a_commit_changed_some_files() {
        assert!(!changed_in(&repo(), "HEAD").is_empty());
    }

    #[test]
    fn a_range_lists_changed_files() {
        // HEAD~1..HEAD changed at least the files of the last commit.
        assert!(!changed_between(&repo(), "HEAD~1", "HEAD").is_empty());
    }

    #[test]
    fn commits_lists_history() {
        let c = commits(&repo(), &["-n".to_owned(), "3".to_owned()]);
        assert!(!c.is_empty());
        assert!(c.iter().all(|(h, d, s)| {
            !h.is_empty() && !d.is_empty() && !s.is_empty()
        }));
    }

    #[test]
    fn working_and_staged_listers_run() {
        // A clean tree yields empty, but the code path executes.
        let _ = changed_working(&repo(), "HEAD");
        let _ = staged(&repo());
    }

    #[test]
    fn subject_has_a_hash_and_text() {
        let s = subject(&repo(), "HEAD").unwrap_or_default();
        assert!(s.contains(' '), "hash + subject: {s:?}");
    }

    #[cfg(feature = "bpe")]
    #[test]
    fn bpe_tier_counts_a_blob() {
        assert!(count_at(&repo(), "HEAD", "Cargo.toml", true).unwrap_or(0) > 0);
    }
}
