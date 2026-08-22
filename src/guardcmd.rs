//! The `guard` verb: the runtime gate (V52/V53).
//!
//! An ADAPTER, not a daemon. One process per call, no server, no thread,
//! no state. The harness shape lives in `hook`; everything here takes a
//! harness-agnostic `Request` and answers from `.context-policy` alone.
//!
//! ENFORCEMENT NEVER SELF-ENABLES (V53). No policy file means allow, in
//! silence, every time -- with no `.context-policy` and no installed hook,
//! itok is exactly as report-only as it is without this module. That is
//! not a courtesy; it is what keeps the gate set closed.
//!
//! CONSERVATIVE, and deterministic (V5/V36). The estimate uses the pinned
//! local tier, never `--ollama`: a gate that varies per run is not a gate,
//! and the job is catching overload rather than reporting exact tokens.
//!
//! PINS ARE ABSOLUTE (V56). A pinned path is allowed before any budget is
//! consulted, because a guard that elides the rules it guards under is
//! self-defeating.
//!
//! What this build does NOT honor, it says (V105): `fuse` tiers and the
//! rate fuse parse but do not decide yet, so a policy containing them
//! fails loudly rather than running with half its rules asleep.

use crate::cli::Output;
use crate::hook::{Decision, Request};
use crate::policy::Policy;
use std::path::Path;

/// `itok guard` -- hook JSON on stdin, decision JSON on stdout.
pub(crate) fn guard(stdin: &str) -> Output {
    let req = crate::hook::request(stdin);
    let root = req.cwd.clone().unwrap_or_else(|| ".".to_owned());
    match crate::policy::read(Path::new(&root)) {
        Ok(p) => decide_or_refuse(&p, &req, Path::new(&root)),
        // A malformed policy does NOT become an allow: the author wrote
        // rules believing they run (V88). It is the adapter that failed,
        // so it says so on stderr and exits non-zero -- a different claim
        // from any decision, which always travels in the JSON (V52).
        Err(e) => Output::usage(format!("itok: {e}\n")),
    }
}

fn decide_or_refuse(p: &Policy, req: &Request, root: &Path) -> Output {
    let unhonored = crate::policy::unhonored(p);
    if let Some(first) = unhonored.first() {
        return Output::usage(format!("itok: {first}\n"));
    }
    let (d, why) = decide(p, req, root);
    Output::ok(crate::hook::response(d, &why))
}

/// The decision, in precedence order. Each step is named because the
/// ORDER is the rule: a pin that lost to a budget would not be absolute.
fn decide(p: &Policy, req: &Request, root: &Path) -> (Decision, String) {
    if p.is_empty() {
        return (Decision::Allow, String::new()); // V53
    }
    if let Some(path) = req.path.as_deref()
        && pinned(p, root, path)
    {
        return (Decision::Allow, String::new()); // V56, absolute
    }
    over_budget(p, req, root)
        .map_or((Decision::Allow, String::new()), |m| (Decision::Deny, m))
}

/// A pin matches by the same patterns a budget does, so an author has one
/// matching rule to learn rather than two (V57: not a third language).
fn pinned(p: &Policy, root: &Path, path: &str) -> bool {
    let rel = relative(root, path);
    p.pins.iter().any(|pin| crate::glob::matches(pin, &rel))
}

/// The first rule this call breaks, as the message the harness shows.
fn over_budget(p: &Policy, req: &Request, root: &Path) -> Option<String> {
    let cost = req.path.as_deref().map(|f| cost_of(root, f))?;
    let path = req.path.as_deref()?;
    let rel = relative(root, path);
    tool_breach(p, req, cost).or_else(|| glob_breach(p, &rel, cost))
}

/// Per-tool budget: the harness's own tool name, matched exactly. A tool
/// name is not a path, so it gets no wildcards -- `Read` is `Read`.
fn tool_breach(p: &Policy, req: &Request, cost: u64) -> Option<String> {
    let b = p.tools.iter().find(|b| b.what == req.tool)?;
    (cost > b.tokens).then(|| {
        format!(
            "{} would add ~{cost} itok; tool `{}` is budgeted at {} (.context-policy). {ALTERNATIVE}",
            req.path.as_deref().unwrap_or("this call"),
            b.what,
            b.tokens
        )
    })
}

/// Per-glob budget: the FIRST matching pattern wins, so an author reads
/// the file top-down the way they wrote it.
fn glob_breach(p: &Policy, rel: &str, cost: u64) -> Option<String> {
    let b = p
        .budgets
        .iter()
        .find(|b| crate::glob::matches(&b.what, rel))?;
    (cost > b.tokens).then(|| {
        format!(
            "{rel} is ~{cost} itok; `{}` is budgeted at {} (.context-policy). {ALTERNATIVE}",
            b.what, b.tokens
        )
    })
}

/// A denial NAMES a cheaper route, because a bare refusal costs more than
/// the waste: the agent retries and rephrases, burning the turns the gate
/// was meant to save (V54's reasoning, which T44 will extend to tiers).
const ALTERNATIVE: &str = "Read a line range, narrow with `rg -m`, or bundle under budget with \
     `itok fit --window N`.";

/// What this path costs, by the pinned local tier (V5/V36). A file that is
/// not there costs nothing and cannot breach -- absence is not a budget
/// problem, and treating it as one would deny a call over a typo.
fn cost_of(root: &Path, path: &str) -> u64 {
    let full = root.join(path);
    let target = if full.exists() {
        full
    } else {
        root.join(relative(root, path))
    };
    crate::walk::bytes(&target).map_or(0, |b| b.saturating_div(4))
}

/// Paths arrive absolute from the harness; patterns are written relative
/// to the repo. Strip the root so the two can meet, and leave anything
/// outside the root alone rather than mangling it.
fn relative(root: &Path, path: &str) -> String {
    Path::new(path)
        .strip_prefix(root)
        .map_or_else(|_| path.to_owned(), |p| p.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(tool: &str, path: Option<&str>, cwd: &str) -> Request {
        Request {
            tool: tool.to_owned(),
            path: path.map(str::to_owned),
            cwd: Some(cwd.to_owned()),
        }
    }

    /// A JSON string literal, so a temp-dir path with a quote in it does
    /// not emit a payload no parser accepts.
    fn quoted(p: &std::path::Path) -> String {
        format!("\"{}\"", crate::json::escape(&p.display().to_string()))
    }

    /// A `Read` payload naming one file under `root`.
    fn read_payload(root: &std::path::Path, file: &str) -> String {
        format!(
            r#"{{"tool_name":"Read","cwd":{},"tool_input":{{"file_path":"{file}"}}}}"#,
            quoted(root)
        )
    }

    fn policy(text: &str) -> Policy {
        match crate::policy::parse(text) {
            Ok(p) => p,
            Err(e) => unreachable!("test policy must parse: {e}"),
        }
    }

    /// A scratch repo with one file of a known size, so a budget can be
    /// crossed deliberately rather than by whatever happens to be on disk.
    fn repo(name: &str, bytes: usize) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("itok-guard-{name}"));
        let _ = std::fs::create_dir_all(dir.join("src"));
        let _ = std::fs::write(dir.join("src/big.rs"), "x".repeat(bytes));
        dir
    }

    /// V53, the invariant that matters most here: no policy means allow,
    /// silently, forever. Enforcement never self-enables.
    #[test]
    fn an_empty_policy_allows_in_silence() {
        let root = repo("empty", 400);
        let (d, why) = decide(
            &Policy::default(),
            &req("Read", Some("src/big.rs"), "."),
            &root,
        );
        assert_eq!(d, Decision::Allow);
        assert!(why.is_empty(), "nothing to say");
    }

    /// A budget that is crossed denies, and the reason names the cheaper
    /// route rather than just refusing (V54).
    #[test]
    fn a_crossed_glob_budget_denies_and_names_an_alternative() {
        let root = repo("glob", 4_000); // ~1000 itok
        let p = policy("budget src/**/*.rs 100\n");
        let (d, why) = decide(&p, &req("Read", Some("src/big.rs"), "."), &root);
        assert_eq!(d, Decision::Deny, "{why}");
        assert!(why.contains("itok fit"), "names a cheaper route: {why}");
        assert!(why.contains("src/**/*.rs"), "names the rule: {why}");
    }

    /// Under budget is an allow, and a silent one.
    #[test]
    fn a_budget_that_is_not_crossed_allows() {
        let root = repo("under", 40); // ~10 itok
        let p = policy("budget src/**/*.rs 100\n");
        let (d, why) = decide(&p, &req("Read", Some("src/big.rs"), "."), &root);
        assert_eq!(d, Decision::Allow);
        assert!(why.is_empty());
    }

    /// V56: a pin wins over a budget that the same path would break. The
    /// ORDER is the invariant, so this is checked with both rules live.
    #[test]
    fn a_pin_beats_a_budget_the_path_would_break() {
        let root = repo("pin", 4_000);
        let p = policy("budget src/**/*.rs 100\npin src/big.rs\n");
        let (d, _) = decide(&p, &req("Read", Some("src/big.rs"), "."), &root);
        assert_eq!(d, Decision::Allow, "a pin is absolute");
    }

    /// Per-tool budgets match the tool name exactly -- a name is not a
    /// path, so it gets no wildcards.
    #[test]
    fn a_tool_budget_matches_the_tool_by_name() {
        let root = repo("tool", 4_000);
        let p = policy("tool Read 100\n");
        let (denied, _) =
            decide(&p, &req("Read", Some("src/big.rs"), "."), &root);
        assert_eq!(denied, Decision::Deny);
        let (other, _) =
            decide(&p, &req("Edit", Some("src/big.rs"), "."), &root);
        assert_eq!(other, Decision::Allow, "a different tool is untouched");
    }

    /// A call naming no file cannot breach a path budget -- and must not
    /// be denied for it. `Bash` is the common case.
    #[test]
    fn a_call_without_a_path_is_allowed() {
        let root = repo("nopath", 4_000);
        let p = policy("budget src/**/*.rs 100\n");
        let (d, _) = decide(&p, &req("Bash", None, "."), &root);
        assert_eq!(d, Decision::Allow);
    }

    /// V105: a policy carrying rules this build cannot honor FAILS, rather
    /// than deciding with half its rules asleep.
    #[test]
    fn a_policy_with_unhonored_rules_refuses_to_decide() {
        let out = decide_or_refuse(
            &policy("fuse warn 70\n"),
            &req("Read", None, "."),
            Path::new("."),
        );
        assert_eq!(out.code, 2, "the adapter failed, it did not decide");
        assert!(out.err.contains("T44"), "{}", out.err);
        assert!(out.out.is_empty(), "no decision on stdout");
    }

    /// V88, end to end: a malformed policy is not an allow. The author
    /// wrote rules believing they run.
    #[test]
    fn a_malformed_policy_is_not_an_allow() {
        let dir = std::env::temp_dir().join("itok-guard-bad");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join(".context-policy"), "budget oops\n");
        let payload =
            format!(r#"{{"tool_name":"Read","cwd":{}}}"#, quoted(&dir));
        let out = guard(&payload);
        assert_eq!(out.code, 2, "{:?}", out.out);
        assert!(out.err.contains(".context-policy:1:"), "{}", out.err);
    }

    /// V52: the decision travels in the JSON on stdout, at exit 0 -- the
    /// harness reads stdout, never an exit code.
    #[test]
    fn a_decision_is_json_on_stdout_at_exit_zero() {
        let root = repo("json", 4_000);
        let _ = std::fs::write(
            root.join(".context-policy"),
            "budget src/**/*.rs 100\n",
        );
        let out = guard(&read_payload(&root, "src/big.rs"));
        assert_eq!(out.code, 0, "a denial is still a successful adapter run");
        assert!(
            out.out.contains("\"permissionDecision\":\"deny\""),
            "{}",
            out.out
        );
        assert!(out.err.is_empty(), "nothing on stderr");
    }
}
