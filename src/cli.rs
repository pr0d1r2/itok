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
        None => Output::usage(crate::docs::usage()),
        Some((first, rest)) => head(first, rest),
    }
}

fn head(first: &str, rest: &[String]) -> Output {
    match first {
        "--version" | "-V" => Output::ok(format!("itok {VERSION}\n")),
        "--help" | "-h" => Output::ok(crate::docs::usage()),
        // `docs` is READ-ONLY (V6) but full-name only, not prefix-inferred:
        // it must not steal `doctor`'s `do`/`doc` prefixes. Emits the
        // markdown reference to stdout; `itok docs > README.md` is the
        // user's redirect -- the tool never writes (V40).
        "docs" => Output::ok(crate::docs::markdown()),
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
    let usage = crate::docs::usage();
    format!("itok: '{verb}' is ambiguous: {}\n{usage}", cands.join(", "))
}

fn unknown_msg(verb: &str, near: &[&str]) -> String {
    let usage = crate::docs::usage();
    if near.is_empty() {
        format!("itok: unknown command '{verb}'\n{usage}")
    } else {
        format!(
            "itok: unknown command '{verb}' -- did you mean {}?\n{usage}",
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
    fn docs_gives_the_markdown_reference() {
        // Full-name verb, read-only to stdout (V40): markdown out, no stderr.
        let o = run(&args(&["docs"]));
        assert_eq!(o.code, 0);
        assert!(o.out.starts_with("## Commands"));
        assert!(o.out.contains("### `estimate`"));
        assert!(o.err.is_empty(), "read-only: {:?}", o.err);
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
