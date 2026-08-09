//! gitref: the token cost of a file AT a git commit -- the shared
//! primitive under diff/show/log (V33). Each of those reads blobs at refs
//! and counts them, so the git plumbing lives here once: blob content at
//! a ref, the files a commit or a range touched, and a commit's subject.

use std::path::Path;

/// Every variable through which an AMBIENT repository can reach a child
/// `git`, and the reason `-C` alone is not isolation.
const GIT_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_CEILING_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_PREFIX",
];

/// A `git` bound to `root` and DEAF to whatever repository invoked us.
///
/// `-C` sets the working DIRECTORY; the variables above set the git
/// DIRECTORY, and the environment WINS. Git exports them into everything it
/// runs -- a hook, `rebase -x`, `filter-branch` -- and itok RUNS FROM A
/// HOOK: this repo's own gate calls `itok check` from `pre-commit`, and
/// `guard` is a harness hook by design. Unscrubbed, every verb answers
/// about the invoking repo, or about nothing at all, with no diagnostic
/// (B19).
///
/// It looked correct here only because the two paths coincide in this repo.
/// `itok -C /other/repo show HEAD` from a hook read the WRONG repository,
/// and the failure is silent -- which is this codebase's dominant defect
/// shape (B7, B11, B14, B18).
pub(crate) fn git(root: &Path) -> std::process::Command {
    let mut cmd = std::process::Command::new("git");
    cmd.arg("-C").arg(root);
    for key in GIT_ENV {
        cmd.env_remove(key);
    }
    cmd
}

/// A file's content at `commit` (`git cat-file -p commit:path`), or None
/// if the path did not exist there or git errored.
pub(crate) fn blob(root: &Path, commit: &str, path: &str) -> Option<String> {
    let spec = format!("{commit}:{path}");
    let out = git(root).args(["cat-file", "-p", &spec]).output().ok()?;
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

/// Files a single commit changed, measured against its FIRST parent.
///
/// It was a bare `git diff-tree <commit>`, and that command answers EMPTY
/// -- exit 0, no diagnostic -- for the two commit shapes with no single
/// obvious parent: a MERGE, where git declines to pick a side, and the ROOT
/// commit, where there is no side to pick. So `itok show <merge>` reported
/// a confident zero for a commit that changed plenty (B18). Silent-empty is
/// this repo's dominant defect (B7, B11, B14), and V104 is the rule: a
/// named source that cannot be read is an ERROR, never an empty result.
///
/// FIRST-parent semantics for the merge case, `<commit>^1..<commit>`, which
/// answers "what did this merge bring in". `showcmd` already computes its
/// per-file deltas against `<commit>~1` -- the same commit -- so the file
/// list and the numbers beside it now come from one choice rather than two.
///
/// `-m --first-parent` on `diff-tree` was the other candidate and is WRONG:
/// measured on a three-commit fixture it reports the mainline commit's own
/// files alongside the merged ones. A quietly wrong answer is worse than the
/// empty one it replaces.
///
/// An unresolvable ref still yields empty. That is the same silent-empty
/// class one level along, and it is NOT fixed here: `Vec<String>` has no
/// error channel, so honouring V104 for a bad ref is a signature change
/// through `showcmd` and is its own task, not a rider on this one.
pub(crate) fn changed_in(root: &Path, commit: &str) -> Vec<String> {
    let Some(n) = parents(root, commit) else {
        return Vec::new();
    };
    if n == 0 {
        return root_diff(root, commit);
    }
    let first = format!("{commit}^1");
    names(root, &["diff", "--name-only", &first, commit])
}

/// The root commit's files. `--root` is git's own spelling for "diff this
/// against nothing", which is why no empty-tree hash is written down here.
fn root_diff(root: &Path, commit: &str) -> Vec<String> {
    names(
        root,
        &[
            "diff-tree",
            "--root",
            "--no-commit-id",
            "--name-only",
            "-r",
            commit,
        ],
    )
}

/// How many parents `commit` has, or None when git cannot resolve it at all
/// -- which is a different thing from a commit with zero parents, and the
/// distinction is the whole point of the Option.
fn parents(root: &Path, commit: &str) -> Option<usize> {
    let out = git(root)
        .args(["rev-list", "--parents", "-n", "1", commit])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    // `<sha> <parent>...` -- the commit's own hash is the first field, so
    // the parent count is one less. checked_sub, because empty output must
    // read as unresolvable rather than wrap to a huge count.
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .count()
        .checked_sub(1)
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
    let Ok(out) = git(root).args(args).output() else {
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
    git(root)
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
    let out = git(root)
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

    /// Bring a scratch repo into existence. Separate from the steps below
    /// so neither list grows past what a reader can hold at once.
    const FIXTURE_INIT: &[&[&str]] = &[
        &["init", "-q", "."],
        &["config", "user.email", "fixture@example.invalid"],
        &["config", "user.name", "fixture"],
    ];

    /// `(file to create first, the git command)`. The shape is the point:
    /// a root commit, a side branch, a mainline commit, and a `--no-ff`
    /// merge -- one real commit for every branch of `changed_in`.
    const FIXTURE_STEPS: &[(&str, &[&str])] = &[
        ("root.txt", &["commit", "-qm", "root"]),
        ("", &["checkout", "-qb", "side"]),
        ("side.txt", &["commit", "-qm", "side"]),
        ("", &["checkout", "-q", "-"]),
        ("main.txt", &["commit", "-qm", "mainline"]),
        ("", &["merge", "-q", "--no-ff", "side", "-m", "merge"]),
    ];

    // Repo root from git (V37): the monorepo root in-tree, the crate dir in
    // the extraction rehearsal (T11) -- one test, both layouts.
    fn repo() -> PathBuf {
        PathBuf::from(crate::testutil::repo_root())
    }

    #[test]
    fn blob_reads_a_file_at_head() {
        if !crate::testutil::dogfood() {
            return;
        }
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
        if !crate::testutil::dogfood() {
            return;
        }
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
        if !crate::testutil::dogfood() {
            return;
        }
        assert!(!changed_in(&repo(), "HEAD").is_empty());
    }

    /// A scratch repo whose shape is the POINT: a root commit, a mainline
    /// commit, a side branch, and a `--no-ff` merge -- so every branch of
    /// `changed_in` has a real commit to run against (V79: the guard stays
    /// armed where the data lives).
    ///
    /// Built rather than recorded, because a merge cannot be faked with a
    /// fixture file, and this repo has none of its own: `main` is linear and
    /// branch protection now REQUIRES it to stay that way. The one commit
    /// shape itok most needs to handle is the one its own history can never
    /// contain, which is exactly how B18 reached a published tool.
    ///
    /// This is the first test here to need a WORKING `git`, not merely a
    /// git-shaped answer -- every other one passes when git errors, because
    /// erroring is what they assert. Stated out loud because an unannounced
    /// environment requirement in the shipped suite is what B17 was.
    fn merge_fixture(name: &str) -> PathBuf {
        let dir = fixture_dir(name);
        for (file, args) in FIXTURE_STEPS {
            if !file.is_empty() {
                let w = std::fs::write(dir.join(file), "x\n");
                assert!(w.is_ok(), "fixture write {file}");
                // The named file, never `add -A`. If isolation ever fails
                // again, one stray path beats every file in the repo.
                assert!(fgit(&dir, &["add", "--", file]), "fixture add {file}");
            }
            assert!(fgit(&dir, args), "fixture step {args:?}");
        }
        dir
    }

    /// A clean directory with an initialised repo and a committer identity.
    /// The identity is set here rather than inherited, so the fixture does
    /// not depend on the machine's git config -- a committer with no
    /// `user.email` cannot commit at all, and that failure would read as a
    /// bug in `changed_in`.
    fn fixture_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("itok-gitref-{name}"));
        assert!(dir.is_absolute(), "fixture path must be absolute: {dir:?}");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(std::fs::create_dir_all(&dir).is_ok(), "fixture dir");
        for args in FIXTURE_INIT {
            assert!(fgit(&dir, args), "fixture needs a working `git`");
        }
        assert_own_repo(&dir);
        dir
    }

    /// THE guard, and the reason this fixture is safe to run from a git hook.
    ///
    /// If any ambient `GIT_*` variable survives the scrub, the fixture's
    /// commits land in the AMBIENT repository -- which is exactly what
    /// happened once here: four commits and a moved branch in the repo under
    /// test. Comparing the fresh repo's own toplevel against the directory we
    /// asked for turns that into one red test instead.
    ///
    /// Canonicalised because macOS resolves `/var` to `/private/var`, so the
    /// two spellings name one directory and a raw comparison would fail on a
    /// correct fixture.
    fn assert_own_repo(dir: &Path) {
        let top =
            PathBuf::from(fgit_out(dir, &["rev-parse", "--show-toplevel"]));
        let (want, got) = (dir.canonicalize().ok(), top.canonicalize().ok());
        assert!(
            want.is_some() && want == got,
            "fixture must be its OWN repo -- wanted {want:?}, git says \
             {got:?}. An ambient GIT_DIR/GIT_WORK_TREE leaked past the \
             scrub, and the commits would have gone into the repo under test."
        );
    }

    /// The fixture uses the SAME scrubbed constructor the tool does.
    ///
    /// The first draft used a plain `git -C <tmp>` and destroyed the branch
    /// under test: hk runs from `pre-commit`, git had exported `GIT_DIR` and
    /// `GIT_INDEX_FILE`, those are inherited all the way down to a test
    /// process, and they override `-C`. The fixture read its files from the
    /// temp dir and wrote its commits into the real repo -- four commits, the
    /// branch moved, and a tree holding nothing but the three fixture files.
    /// `-C` was never the isolation it looks like, which is B19 in one line.
    fn fgit(dir: &Path, args: &[&str]) -> bool {
        git(dir)
            .args(args)
            .output()
            .is_ok_and(|o| o.status.success())
    }

    /// git stdout from a scrubbed invocation, trimmed.
    fn fgit_out(dir: &Path, args: &[&str]) -> String {
        git(dir)
            .args(args)
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_default()
    }

    /// THE regression. `git diff-tree <merge>` answers empty, and CI is
    /// where that bites first: for a `pull_request` GitHub checks out an
    /// EPHEMERAL merge of the branch into the base, so HEAD is a merge
    /// commit on every PR run. The repo had no PRs until the day this was
    /// found, which is why a linear-history project shipped the bug.
    #[test]
    fn a_merge_commit_reports_what_it_merged() {
        let dir = merge_fixture("merge");
        let changed = changed_in(&dir, "HEAD");
        assert_eq!(
            changed,
            vec!["side.txt".to_owned()],
            "first-parent semantics: the merge brought in side.txt, and \
             main.txt was already on the first parent"
        );
    }

    /// The second silent-empty shape, found while measuring the first.
    /// Nothing brought it up -- it was caught by asking which OTHER commits
    /// have no single parent, which is the question B11 says to ask.
    #[test]
    fn the_root_commit_reports_everything_it_added() {
        let dir = merge_fixture("root");
        let root = fgit_out(&dir, &["rev-list", "--max-parents=0", "HEAD"]);
        assert!(!root.is_empty(), "fixture must have a root commit");
        assert_eq!(changed_in(&dir, &root), vec!["root.txt".to_owned()]);
    }

    /// The ordinary case still behaves -- a fix for two edge shapes that
    /// broke the common one would be a poor trade, and nothing else here
    /// pins it against a repo whose contents are known.
    #[test]
    fn an_ordinary_commit_is_unchanged_by_the_fix() {
        let dir = merge_fixture("ordinary");
        assert_eq!(
            changed_in(&dir, "HEAD^1"),
            vec!["main.txt".to_owned()],
            "the mainline commit, diffed against its one parent"
        );
    }

    /// A ref git cannot resolve is still empty, and that is DELIBERATE
    /// here: `parents` returns None and `changed_in` has no error channel
    /// to report it through. Pinned so the limit is a recorded decision
    /// rather than an assumption someone later reads as coverage.
    #[test]
    fn an_unresolvable_ref_is_empty_for_now() {
        assert!(changed_in(&repo(), "no-such-ref-xyz").is_empty());
    }

    #[test]
    fn a_range_lists_changed_files() {
        if !crate::testutil::dogfood() {
            return;
        }
        // HEAD~1..HEAD changed at least the files of the last commit.
        assert!(!changed_between(&repo(), "HEAD~1", "HEAD").is_empty());
    }

    #[test]
    fn commits_lists_history() {
        if !crate::testutil::dogfood() {
            return;
        }
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
        if !crate::testutil::dogfood() {
            return;
        }
        let s = subject(&repo(), "HEAD").unwrap_or_default();
        assert!(s.contains(' '), "hash + subject: {s:?}");
    }

    #[cfg(feature = "bpe")]
    #[test]
    fn bpe_tier_counts_a_blob() {
        if !crate::testutil::dogfood() {
            return;
        }
        assert!(count_at(&repo(), "HEAD", "Cargo.toml", true).unwrap_or(0) > 0);
    }
}
