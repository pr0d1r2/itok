//! File selection: git-tracked by default (V8), like `rg` respects
//! `.gitignore`. Reimplemented here rather than imported from the host's
//! `repo-guard` -- zero host deps (V13), so this survives extraction.

use std::path::Path;

/// Tracked files under `root`, read from the git index -- never a
/// filesystem walk. Empty if `root` is not a git repo (git errored).
#[must_use]
pub fn tracked(root: &Path) -> Vec<String> {
    // The scrubbed constructor, not a bare `Command`: `-C` does not override
    // an inherited GIT_DIR, and `itok check` runs from `pre-commit` in this
    // repo's own gate, where git has exported one (B19). Unscrubbed, the
    // "tracked set" is the INVOKING repo's, silently.
    let Ok(out) = crate::gitref::git(root).args(["ls-files", "-z"]).output()
    else {
        return Vec::new();
    };
    out.stdout
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect()
}

/// Byte length of a file, or None if it cannot be read.
#[must_use]
pub fn bytes(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|m| m.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_reads_a_real_file() {
        // This crate's own manifest -- present, nonempty, no temp dir.
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        assert!(bytes(&p).unwrap_or(0) > 0);
    }

    #[test]
    fn bytes_of_a_missing_file_is_none() {
        assert_eq!(bytes(Path::new("/no/such/itok/file")), None);
    }

    #[test]
    fn tracked_lists_this_repo() {
        // Git repo root at runtime (V37): works in-tree and extracted (T11).
        let root = crate::testutil::repo_root();
        assert!(
            !tracked(Path::new(&root)).is_empty(),
            "expected tracked files"
        );
    }

    #[test]
    fn tracked_of_a_non_repo_is_empty() {
        // git -C on a missing path errors -> the else branch -> empty.
        assert!(tracked(Path::new("/no/such/itok/repo")).is_empty());
    }
}
