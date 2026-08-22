//! The CLI as a pure function: args in, `Output` (streams + exit code)
//! out. No I/O here -- `main` prints the strings and sets the code. This
//! is what makes the whole command surface unit- and property-testable
//! at lib cost, instead of spawning the binary: V245-style totality
//! ("never panics on any input") becomes a proptest, not a hope.
//!
//! Dispatch spine only: argv -> a verb -> its command module. Parsing
//! lives in `args`, each verb's orchestration in its own module
//! (`estcmd` for `estimate`). Split at the byte ceiling (V483).

use crate::verb::{Resolution, Verb, resolve};

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

    /// A usage error `msg` followed by the command reference (V40): the
    /// one place the "message + usage" shape lives, so every verb's error
    /// path stays a short single line.
    pub(crate) fn usage_err(msg: String) -> Self {
        Self::usage(format!("{msg}\n{}", crate::docs::usage()))
    }

    /// A budget breach: the report still prints, the note goes to stderr,
    /// exit 1 (V16).
    pub(crate) fn breach(out: String, err: String) -> Self {
        Self { out, err, code: 1 }
    }
}

/// Where a filter verb's input comes from. A closure, not a `String`, so
/// the process's stdin is touched ONLY when the resolved verb actually
/// reads it -- and so a test can drive `cap` with fixed input instead of
/// whatever the test runner happens to attach. Without that, the totality
/// proptest below would block on a terminal the moment it generated `cap`.
pub type Input<'a> = &'a dyn Fn() -> String;

/// Run itok over `args` (argv without the program name), reading stdin if
/// the verb calls for it.
#[must_use]
pub fn run(args: &[String]) -> Output {
    run_with(args, &stdin_text)
}

/// The same run, over a supplied input source (V89: still a pure function
/// of its inputs -- the impurity is now the caller's to hand in).
#[must_use]
pub fn run_with(args: &[String], input: Input) -> Output {
    match args.split_first() {
        None => Output::usage(crate::docs::usage()),
        Some((first, rest)) => head(first, rest, input),
    }
}

/// stdin as lossy UTF-8. A filter takes whatever the pipe hands it: a
/// binary blob is counted and passed through, never a crash and never an
/// error the caller did not ask for (V49 -- `cmd | itok cap 10k` must work
/// on any `cmd`).
fn stdin_text() -> String {
    let mut buf = Vec::new();
    let _ = std::io::Read::read_to_end(&mut std::io::stdin().lock(), &mut buf);
    String::from_utf8_lossy(&buf).into_owned()
}

fn head(first: &str, rest: &[String], input: Input) -> Output {
    match first {
        "--version" | "-V" => Output::ok(format!("itok {VERSION}\n")),
        "--help" | "-h" => Output::ok(crate::docs::usage()),
        // `docs` is READ-ONLY (V6) but full-name only, not prefix-inferred:
        // it must not steal `doctor`'s `do`/`doc` prefixes. Emits the
        // markdown reference to stdout; `itok docs > README.md` is the
        // user's redirect -- the tool never writes (V40).
        "docs" => Output::ok(crate::docs::markdown()),
        verb => run_verb(verb, rest, input),
    }
}

fn run_verb(verb: &str, rest: &[String], input: Input) -> Output {
    handle(resolve(verb), verb, rest, input)
}

/// Map a resolution to an Output. Split from `run_verb` so every arm --
/// including Ambiguous, which `resolve` cannot yet produce with one verb
/// -- is unit-testable without faking a second verb.
fn handle(
    res: Resolution,
    verb: &str,
    rest: &[String],
    input: Input,
) -> Output {
    match res {
        Resolution::Verb(v) => dispatch(v, rest, input),
        Resolution::Ambiguous(c) => Output::usage(ambiguous_msg(verb, &c)),
        Resolution::Unknown(n) => Output::usage(unknown_msg(verb, &n)),
    }
}

/// Route a resolved verb to its command. Exhaustive over `Verb` -- a new
/// verb is a compile error here until it is wired.
/// The runtime-axis verbs, together because they share a feature gate
/// (V23's shape: a tier that needs a dep is opt-in).
/// `rate` takes the input CLOSURE, not its result: only `--statusline`
/// reads a payload, and calling `input()` here would make every bare
/// `itok rate` block on whatever stdin happens to be -- a terminal, under
/// plain `cargo test`. That is the same hazard the `Input` type exists to
/// prevent, one verb further down.
#[cfg(feature = "session")]
fn runtime(v: Verb, rest: &[String], input: Input) -> Output {
    match v {
        Verb::Top => crate::topcmd::top(rest),
        Verb::Headroom => crate::headroom::headroom(rest),
        Verb::Calibrate => crate::calibrate::calibrate(rest),
        Verb::Rate => crate::ratecmd::rate(rest, input),
        _ => crate::tracecmd::trace(rest),
    }
}

#[cfg(not(feature = "session"))]
fn runtime(_v: Verb, _rest: &[String], _input: Input) -> Output {
    Output::usage_err(
        "itok: the runtime verbs need the `session` feature".to_owned(),
    )
}

fn dispatch(v: Verb, rest: &[String], input: Input) -> Output {
    match v {
        Verb::Cap => crate::capcmd::cap(rest, &input()),
        Verb::Guard => crate::guardcmd::guard(&input()),
        Verb::Estimate => crate::estcmd::estimate(rest),
        Verb::Doctor => crate::doctor::doctor(rest),
        Verb::Diff => crate::diffcmd::diff(rest),
        Verb::Show => crate::showcmd::show(rest),
        Verb::Log => crate::logcmd::log(rest),
        Verb::Check => crate::checkcmd::check(rest),
        Verb::Fit => crate::fitcmd::fit(rest),
        _ => runtime(v, rest, input),
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

    /// An empty input source: the tests that are not about `cap` must
    /// never reach for the runner's stdin.
    fn nothing() -> String {
        String::new()
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
        let o = handle(res, "d", &[], &nothing);
        assert_eq!(o.code, 2);
        assert!(o.err.contains("ambiguous"));
        assert!(o.err.contains("diff") && o.err.contains("doctor"));
    }

    /// The wildcard in `runtime` routes anything unmatched to `trace`, so
    /// a runtime verb that lost its arm would silently BECOME trace and
    /// still exit 0. Each one is pinned to output only its own module
    /// produces.
    #[cfg(feature = "session")]
    #[test]
    fn each_runtime_verb_reaches_its_own_module() {
        // tool-shapes, not minimal: minimal has no load events, so trace
        // prints nothing there and the assertion would pass vacuously.
        let f = format!("{DIR}/tests/fixtures/session/tool-shapes.jsonl");
        let out = |v: &str| run(&args(&[v, &f, "--format", "json"])).out;
        assert!(out("headroom").contains("\"rate_unit\""), "headroom");
        assert!(out("top").contains("\"summary\":true"), "top");
        assert!(out("trace").contains("\"ts\":"), "trace");
        assert!(out("rate").contains("\"per_hour\":"), "rate");
    }

    /// `cap` is the only verb fed from the input source, and it is fed the
    /// SUPPLIED one -- the wiring a lib test can check without a process.
    #[test]
    fn cap_reads_the_input_source_and_announces_its_cut() {
        let text = || "one\ntwo\nthree\n".to_owned();
        let o = run_with(&args(&["cap", "2"]), &text);
        assert_eq!(o.code, 0);
        assert!(
            o.out.starts_with("one\n"),
            "body passes through: {:?}",
            o.out
        );
        assert!(o.out.contains("[itok cap:"), "footer: {:?}", o.out);
        assert!(o.out.contains("resume:"), "selector: {:?}", o.out);
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
        ///
        /// Driven through `run_with`, not `run`: a generated `cap` would
        /// otherwise read the runner's stdin, which is a terminal under
        /// plain `cargo test` -- a hang that appears only on some seeds is
        /// the flake class V68 forbids suppressing later.
        #[test]
        fn run_never_panics(a in prop::collection::vec(arg(), 0..7)) {
            let o = run_with(&a, &nothing);
            prop_assert!(o.code == 0 || o.code == 1 || o.code == 2);
        }
    }
}
