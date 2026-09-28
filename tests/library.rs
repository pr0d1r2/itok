//! The library surface a consumer reaches from OUTSIDE the crate (#41).
//! An integration test, not a unit test, because `pub(crate)` passes a
//! unit test and fails here -- visibility is only provable from outside.

use itok::estimate::select_paths;
use itok::glob::matches;
use std::path::Path;

/// The three wildcards only, NOT gitignore's surface rules: a pattern
/// without `/` is anchored at the root, so `*.md` does not reach a
/// nested file. This is the documented contract, pinned.
#[test]
fn glob_is_the_three_wildcards_not_gitignore() {
    assert!(matches("*.md", "README.md"));
    assert!(!matches("*.md", "docs/guide.md"));
    assert!(matches("**/*.md", "docs/guide.md"));
    assert!(matches("src/?.rs", "src/a.rs"));
    assert!(!matches("src/*", "src/a/b.rs"));
}

#[test]
fn select_paths_refuses_a_directory_and_a_missing_path() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = select_paths(&["src".to_owned()], root);
    assert!(dir.is_err_and(|e| e.contains("directory")));
    let gone = select_paths(&["no/such/file".to_owned()], root);
    assert!(gone.is_err_and(|e| e.contains("no/such/file")));
    let file = select_paths(&["Cargo.toml".to_owned()], root);
    assert_eq!(file, Ok(vec!["Cargo.toml".to_owned()]));
}
