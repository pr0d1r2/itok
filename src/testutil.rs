//! Shared test helpers that keep the git-command tests LAYOUT-AGNOSTIC
//! (V37): the enclosing repo root and this crate's path prefix come from
//! git at RUNTIME, so one test passes both at `<repo>/crates/itok` and when
//! the crate is the extracted repo root. Hardcoding `parent().parent()` as
//! the root or a `crates/itok/...` pathspec couples a test to the monorepo
//! layout and breaks the extraction rehearsal (B2/T11).

const DIR: &str = env!("CARGO_MANIFEST_DIR");

/// A host for tests that need one, defined ONCE so no test carries a real
/// address. `192.0.2.x` is TEST-NET-1 (RFC 5737), reserved for
/// documentation and guaranteed never routable -- so this cannot become a
/// real machine later, which a plausible-looking LAN address can.
///
/// NOT read from `OLLAMA_HOST`, deliberately, and the distinction matters.
/// That variable is the RUNTIME resolution chain (V22) and belongs in
/// `ollama::hosts`. A test that read it would take its input from the
/// machine it happens to run on, which is exactly the ambient-state
/// coupling B3 was about -- the same test would then assert different
/// things on two developers' laptops. These tests want a fixed string; what
/// they check is an identity (`--ollama=X` yields host `X`) and a
/// difference (two hosts must render differently), and neither depends on
/// the value being anyone's real endpoint.
pub(crate) const HOST_IP: &str = "192.0.2.10";

/// A second address, for the tests that must show two hosts differing.
pub(crate) const HOST_IP_ALT: &str = "192.0.2.20";

/// A bare hostname, for the `=`-form and default-port paths. `.invalid` is
/// reserved by RFC 2606 and can never resolve.
pub(crate) const HOST_NAME: &str = "tokhost.invalid";

/// Whether the DOGFOOD tests may run: the ones that read THIS repo's own
/// git history -- tracked set, blobs at HEAD, `HEAD~1..HEAD` -- rather than
/// a fixture. They need a git repo WITH history at `CARGO_MANIFEST_DIR`,
/// and the published `.crate` is neither: a consumer's registry source dir
/// has no `.git` at all. `cargo package --verify` never caught it because
/// it only COMPILES the tarball, so the miss survived to a dry run (B17).
///
/// An ENV gate, not a silent `is this a repo` probe, and the difference is
/// the whole point. Auto-skipping would go green in the repo too, the day
/// something breaks git detection -- a suite that reports success for the
/// reason it should have reported failure. Requiring the variable makes
/// the skip a DECISION someone made: unset means "not the dogfood
/// environment", and the repo's own gate sets it (`hk.pkl`, `flake.nix`).
///
/// Vendorers are why the gate cannot key off repo presence either: a
/// consumer who vendors this source INTO their git repo would satisfy any
/// such probe and then fail on our history. They will never set this.
pub(crate) fn dogfood() -> bool {
    std::env::var_os("ITOK_DOGFOOD").is_some()
}

/// The enclosing git repo's root (`git rev-parse --show-toplevel`): the
/// monorepo root in-tree, the crate dir once extracted.
pub(crate) fn repo_root() -> String {
    git(&["rev-parse", "--show-toplevel"])
}

/// A repo-root-relative path to one of this crate's own files:
/// `crates/itok/SPEC.md` in the monorepo, `SPEC.md` when extracted
/// (`git rev-parse --show-prefix` yields the crate's prefix, empty at the
/// repo root).
pub(crate) fn crate_path(rel: &str) -> String {
    format!("{}{rel}", git(&["rev-parse", "--show-prefix"]))
}

/// The tool's own scrubbed constructor, not a bare `Command`: `-C` loses to
/// an inherited `GIT_DIR`, and this suite runs from `pre-commit`, where git
/// has exported one (B19). Unscrubbed, `repo_root` answers with the
/// INVOKING repository and every caller inherits the wrong answer.
fn git(args: &[&str]) -> String {
    crate::gitref::git(std::path::Path::new(DIR))
        .args(args)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The gate's own guard, in the direction that cannot misfire.
    ///
    /// It asserts the CONVERSE of what the dogfood tests assume: if someone
    /// asked for them, the environment must actually be able to run them.
    /// So a gate set in a tree with no git history fails LOUDLY here once,
    /// instead of fourteen times with fourteen unrelated messages.
    ///
    /// Checking the other direction -- "in a repo, therefore the gate must
    /// be set" -- is the version that breaks a vendorer, who is inside
    /// THEIR repo and has set nothing. That asymmetry is the reason this
    /// test only has one arm.
    #[test]
    fn asking_for_dogfood_requires_a_repo() {
        if !dogfood() {
            return;
        }
        assert!(
            !repo_root().is_empty(),
            "ITOK_DOGFOOD is set but {DIR} is not in a git repo"
        );
    }
}
