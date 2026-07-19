//! The CLI as a pure function: args in, `Output` (streams + exit code)
//! out. No I/O here -- `main` prints the strings and sets the code. This
//! is what makes the whole command surface unit- and property-testable
//! at lib cost, instead of spawning the binary: V245-style totality
//! ("never panics on any input") becomes a proptest, not a hope.
//!
//! Dispatch spine only: argv -> a verb -> its command module. Parsing
//! lives in `args`, each verb's orchestration in its own module
//! (`estcmd` for `estimate`). Split at the byte ceiling (V483).

use crate::verb::{resolve, Resolution, Verb};

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) const USAGE: &str = "\
itok -- context-cost estimator

usage: itok <command> [args]

commands:
  estimate [-s] [-h] [--top N] [--budget N] [--bpe] [--ollama [HOSTS]]
           [--format human|json] [-C dir] [paths...]
                    token cost of files (git-tracked by default)
                    --bpe: real tokenizer (o200k) instead of bytes/4
                    --ollama: exact via a local model's tokenizer (LAN)
  doctor   [--model X] [--window N] [--ollama [HOSTS]] [--format human|json]
           [-C dir] [paths...]
                    advisory health: fit-to-window, balance, noise,
                    estimate confidence. Reports; never gates.
                    --model X: resolve X's encoding via .context-models.
                    --ollama: live model/window discovery across a fleet.
                    HOSTS = host[:port] comma-list, - (stdin), or
                    .context-hosts; else OLLAMA_HOST, else localhost.
  diff     [<A> <B>|<A>..<B>|<ref>] [--staged] [-- <path>]
                    token delta between two points (default working vs
                    HEAD); --exit-code / --budget N to gate.
  show     [<commit>] [-- <path>] | <commit>:<path>
                    one commit's per-file delta (default HEAD); the
                    <commit>:<path> form is a blob's cost at a ref.
  log      <path> [<A>..<B>] [-n N] [--since D] [--reverse]
                    a path's token cost + delta across the commits that
                    touched it. Report-only.
  check    [-C dir] [--format human|json]
                    gate registered paths against .context-limits;
                    exit 1 on any breach.
  fit      --window N [--by size] [--bpe] [--format human|json] [-C dir]
           [paths...]
                    greedy subset of files that fits the token window;
                    emits a pipeable path list (default git-tracked).
";

/// What a run produced: streams + exit code. No process, no I/O.
#[derive(Debug, PartialEq, Eq)]
pub struct Output {
    pub out: String,
    pub err: String,
    pub code: u8,
}

impl Output {
    pub(crate) fn ok(out: String) -> Self {
        Self {
            out,
            err: String::new(),
            code: 0,
        }
    }

    /// A usage error: message to stderr, exit 2.
    pub(crate) fn usage(err: String) -> Self {
        Self {
            out: String::new(),
            err,
            code: 2,
        }
    }

    /// A budget breach: the report still prints, the note goes to stderr,
    /// exit 1 (V16).
    pub(crate) fn breach(out: String, err: String) -> Self {
        Self { out, err, code: 1 }
    }
}

/// Run itok over `args` (argv without the program name).
#[must_use]
pub fn run(args: &[String]) -> Output {
    match args.split_first() {
        None => Output::usage(USAGE.to_owned()),
        Some((first, rest)) => head(first, rest),
    }
}

fn head(first: &str, rest: &[String]) -> Output {
    match first {
        "--version" | "-V" => Output::ok(format!("itok {VERSION}\n")),
        "--help" | "-h" => Output::ok(USAGE.to_owned()),
        verb => run_verb(verb, rest),
    }
}

fn run_verb(verb: &str, rest: &[String]) -> Output {
    handle(resolve(verb), verb, rest)
}

/// Map a resolution to an Output. Split from `run_verb` so every arm --
/// including Ambiguous, which `resolve` cannot yet produce with one verb
/// -- is unit-testable without faking a second verb.
fn handle(res: Resolution, verb: &str, rest: &[String]) -> Output {
    match res {
        Resolution::Verb(v) => dispatch(v, rest),
        Resolution::Ambiguous(c) => Output::usage(ambiguous_msg(verb, &c)),
        Resolution::Unknown(n) => Output::usage(unknown_msg(verb, &n)),
    }
}

/// Route a resolved verb to its command. Exhaustive over `Verb` -- a new
/// verb is a compile error here until it is wired.
fn dispatch(v: Verb, rest: &[String]) -> Output {
    match v {
        Verb::Estimate => crate::estcmd::estimate(rest),
        Verb::Doctor => crate::doctor::doctor(rest),
        Verb::Diff => crate::diffcmd::diff(rest),
        Verb::Show => crate::showcmd::show(rest),
        Verb::Log => crate::logcmd::log(rest),
        Verb::Check => crate::checkcmd::check(rest),
        Verb::Fit => crate::fitcmd::fit(rest),
    }
}

fn ambiguous_msg(verb: &str, cands: &[&str]) -> String {
    format!("itok: '{verb}' is ambiguous: {}\n{USAGE}", cands.join(", "))
}

fn unknown_msg(verb: &str, near: &[&str]) -> String {
    if near.is_empty() {
        format!("itok: unknown command '{verb}'\n{USAGE}")
    } else {
        format!(
            "itok: unknown command '{verb}' -- did you mean {}?\n{USAGE}",
            near.join(", ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const DIR: &str = env!("CARGO_MANIFEST_DIR");

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn version_names_the_tool() {
        let o = run(&args(&["--version"]));
        assert!(o.out.contains("itok"));
        assert_eq!(o.code, 0);
    }

    #[test]
    fn version_short_flag_works() {
        assert_eq!(run(&args(&["-V"])).code, 0);
    }

    #[test]
    fn help_lists_the_commands() {
        let o = run(&args(&["--help"]));
        assert!(o.out.contains("estimate"));
        assert_eq!(o.code, 0);
    }

    #[test]
    fn no_command_is_a_usage_error() {
        let o = run(&[]);
        assert_eq!(o.code, 2);
        assert!(o.err.contains("usage"));
    }

    #[test]
    fn unknown_command_is_a_usage_error() {
        let o = run(&args(&["bogus"]));
        assert_eq!(o.code, 2);
        assert!(o.err.contains("unknown command"));
    }

    #[test]
    fn a_verb_prefix_resolves_and_runs() {
        let o = run(&args(&["e", "-C", DIR, "Cargo.toml"]));
        assert!(o.out.contains("itok"));
        assert_eq!(o.code, 0);
    }

    #[test]
    fn a_typo_suggests_and_does_not_run() {
        let o = run(&args(&["esimate"]));
        assert_eq!(o.code, 2);
        assert!(o.err.contains("did you mean"));
        assert!(o.err.contains("estimate"));
        assert!(o.out.is_empty(), "must not run: {:?}", o.out);
    }

    #[test]
    fn an_ambiguous_resolution_lists_candidates() {
        // resolve() can't yet yield Ambiguous (one verb); handle() maps
        // it, so drive that arm directly with a synthetic resolution.
        let res = Resolution::Ambiguous(vec!["diff", "doctor"]);
        let o = handle(res, "d", &[]);
        assert_eq!(o.code, 2);
        assert!(o.err.contains("ambiguous"));
        assert!(o.err.contains("diff") && o.err.contains("doctor"));
    }

    fn arg() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("estimate".to_owned()),
            Just("-s".to_owned()),
            Just("--top".to_owned()),
            Just("3".to_owned()),
            "[a-z0-9._/-]{0,5}",
            "-{1,2}[a-z]{1,3}",
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]

        /// Totality (V245): no arg vector ever panics; the code is one
        /// itok actually returns.
        #[test]
        fn run_never_panics(a in prop::collection::vec(arg(), 0..7)) {
            let o = run(&a);
            prop_assert!(o.code == 0 || o.code == 1 || o.code == 2);
        }
    }
}
