//! Argument parsing: argv -> `Opts`, pure and I/O-free. Split from
//! cli.rs when the combined module crossed the per-file byte ceiling
//! (V481/V483): parsing (argv -> Opts) and orchestration (Opts ->
//! Output) are different jobs and now live apart.

/// Output shape. Human is cosmetic (may evolve); Json is the stable
/// machine contract (V9).
#[derive(Default, Debug, PartialEq, Eq)]
pub(crate) enum Format {
    #[default]
    Human,
    Json,
}

/// The precision tier a run counts on (V4): the "shape is an enum" that
/// the bool budget (clippy: <= 3 struct bools) points to. Dummy by
/// default; `--bpe`/`--ollama`, each feature-gated, upgrade it.
#[derive(Default, Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum Tier {
    #[default]
    Dummy,
    #[cfg(feature = "bpe")]
    Bpe,
    #[cfg(feature = "ollama")]
    Ollama,
}

/// `estimate` options. Two bools is the struct limit; tier & format are
/// enums, not bools (clippy struct_excessive_bools).
#[derive(Default, Debug, PartialEq, Eq)]
pub(crate) struct Opts {
    pub(crate) summarize: bool,
    pub(crate) human: bool,
    pub(crate) tier: Tier,
    pub(crate) format: Format,
    pub(crate) top: Option<usize>,
    pub(crate) budget: Option<u64>,
    pub(crate) window: Option<u64>,
    pub(crate) model: Option<String>,
    #[cfg(feature = "ollama")]
    pub(crate) ollama_hosts: Option<String>,
    pub(crate) chdir: Option<String>,
    pub(crate) paths: Vec<String>,
}

impl Opts {
    /// Whether the bpe tier is selected -- always false without the
    /// feature, so callers need no `cfg` of their own.
    pub(crate) fn is_bpe(&self) -> bool {
        #[cfg(feature = "bpe")]
        {
            matches!(self.tier, Tier::Bpe)
        }
        #[cfg(not(feature = "bpe"))]
        {
            false
        }
    }

    /// Whether the ollama exact tier is selected.
    #[cfg(feature = "ollama")]
    pub(crate) fn is_ollama(&self) -> bool {
        matches!(self.tier, Tier::Ollama)
    }
}

pub(crate) fn parse(rest: &[String]) -> Result<Opts, String> {
    let mut o = Opts::default();
    let mut i = 0usize;
    while let Some(a) = rest.get(i) {
        apply(&mut o, a, rest, &mut i)?;
        i = i.saturating_add(1);
    }
    Ok(o)
}

/// Apply one token: a no-argument flag, else a value flag or a path.
fn apply(
    o: &mut Opts,
    a: &str,
    rest: &[String],
    i: &mut usize,
) -> Result<(), String> {
    // --ollama optionally takes the next token as its host-list (V24/V25);
    // handled here since it is neither a plain bool nor a required-value
    // flag. Without the feature it falls through to the unknown-flag arm.
    #[cfg(feature = "ollama")]
    if let Some(hosts) = ollama_arg(a) {
        o.tier = Tier::Ollama;
        return set_ollama_hosts(o, hosts, rest, i);
    }
    if boolean(o, a) {
        Ok(())
    } else {
        valued(o, a, rest, i)
    }
}

/// Where `--ollama`'s hosts come from: the `=` value verbatim, or the
/// heuristic over the next token.
///
/// `--ollama=` with nothing after it ERRORS rather than resolving to the
/// default. The user named a value and gave an empty one; falling back to
/// localhost there would be B10's silence in a new place (V101).
#[cfg(feature = "ollama")]
fn set_ollama_hosts(
    o: &mut Opts,
    hosts: Option<&str>,
    rest: &[String],
    i: &mut usize,
) -> Result<(), String> {
    match hosts {
        Some("") => return Err("--ollama= needs a host".to_owned()),
        // Unambiguous, so taken verbatim -- no heuristic, and a bare host
        // reaches the tier (V101).
        Some(h) => o.ollama_hosts = Some(h.to_owned()),
        None => take_ollama_hosts(o, rest, i),
    }
    Ok(())
}

/// Is this token `--ollama`? `Some(None)` for the bare flag, `Some(Some(h))`
/// for the `=` form, `None` when it is some other token.
///
/// The `=` form exists because an OPTIONAL value cannot be disambiguated
/// by looking: `--ollama box1` is the same token sequence whether `box1`
/// is a host or a file (V101). `=` binds the value to the flag by
/// construction, which is why GNU uses it for exactly this case (V1) --
/// and it is the only way to reach the documented bare `host` form, the
/// one the heuristic below must decline (B10).
#[cfg(feature = "ollama")]
fn ollama_arg(a: &str) -> Option<Option<&str>> {
    match a {
        "--ollama" => Some(None),
        _ => a.strip_prefix("--ollama=").map(Some),
    }
}

/// Consume the next token as `--ollama`'s host-list, but ONLY when it looks
/// like hosts -- a comma-list, `host:port`, or `-` (stdin) -- so a bare
/// path (`estimate --ollama SPEC.md`) stays a path, not a host (V24/V25).
#[cfg(feature = "ollama")]
fn take_ollama_hosts(o: &mut Opts, rest: &[String], i: &mut usize) {
    if let Some(next) = rest.get(i.saturating_add(1))
        && (next == "-" || next.contains(',') || next.contains(':'))
    {
        o.ollama_hosts = Some(next.clone());
        *i = i.saturating_add(1);
    }
}

/// The no-argument flags; returns whether `a` was one of them. A gated
/// flag with its feature off is NOT matched here, so it falls through to
/// `valued`'s unknown-flag arm and errors -- never silently ignored (V23).
fn boolean(o: &mut Opts, a: &str) -> bool {
    match a {
        "-s" | "--summarize" => o.summarize = true,
        "-h" | "--human" => o.human = true,
        #[cfg(feature = "bpe")]
        "--bpe" => o.tier = Tier::Bpe,
        _ => return false,
    }
    true
}

/// The value-taking flags, and the path fallback.
fn valued(
    o: &mut Opts,
    a: &str,
    rest: &[String],
    i: &mut usize,
) -> Result<(), String> {
    match a {
        "--format" => o.format = take_format(rest, i)?,
        "--top" => o.top = Some(take_num(rest, i)?),
        "--budget" => o.budget = Some(take_count(rest, i)?),
        "--window" => o.window = Some(take_count(rest, i)?),
        "--model" => o.model = Some(take_val(rest, i)?),
        "-C" => o.chdir = Some(take_val(rest, i)?),
        p if p.starts_with('-') => return Err(unknown(p)),
        p => o.paths.push(p.to_owned()),
    }
    Ok(())
}

/// The diagnostic for an unrecognized flag.
///
/// A flag whose TIER was compiled out is KNOWN, not unknown. Saying
/// `unknown flag '--ollama'` sends the reader hunting for a typo while
/// `--help`, printed directly below it, advertises that very flag -- and
/// `cli.rs` already gets this right for the runtime verbs ("the runtime
/// verbs need the `session` feature"). The rule was simply never carried
/// to the tier flags (B11e/V71: a failure says what to DO).
///
/// Reaching here with a gated flag PROVES its feature is off: with the
/// feature on, `boolean`/`apply` match it first and never fall through.
fn unknown(flag: &str) -> String {
    match gated(flag) {
        Some(f) => format!(
            "{flag} needs the `{f}` feature -- this binary was built \
             without it: rebuild with `--features {f}`"
        ),
        None => format!("unknown flag '{flag}'"),
    }
}

/// The feature a gated flag belongs to.
fn gated(flag: &str) -> Option<&'static str> {
    match flag {
        "--bpe" => Some("bpe"),
        f if f == "--ollama" || f.starts_with("--ollama=") => Some("ollama"),
        _ => None,
    }
}

/// Consume the value after a flag, advancing the cursor.
fn take_val(rest: &[String], i: &mut usize) -> Result<String, String> {
    *i = i.saturating_add(1);
    rest.get(*i)
        .cloned()
        .ok_or_else(|| "flag needs a value".to_owned())
}

fn take_num(rest: &[String], i: &mut usize) -> Result<usize, String> {
    take_val(rest, i)?
        .parse()
        .map_err(|_| "flag needs a number".to_owned())
}

/// A token count with an optional k/M unit (V18, shared parser).
fn take_count(rest: &[String], i: &mut usize) -> Result<u64, String> {
    crate::units::parse(&take_val(rest, i)?)
}

fn take_format(rest: &[String], i: &mut usize) -> Result<Format, String> {
    match take_val(rest, i)?.as_str() {
        "json" => Ok(Format::Json),
        "human" => Ok(Format::Human),
        other => Err(format!("unknown format '{other}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn v(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn flags_set_their_fields() {
        let o = parse(&v(&[
            "-s", "-h", "--top", "5", "--format", "json", "-C", "d", "a", "b",
        ]))
        .ok()
        .unwrap_or_default();
        assert!(o.summarize && o.human);
        assert_eq!(o.top, Some(5));
        assert_eq!(o.format, Format::Json);
        assert_eq!(o.chdir.as_deref(), Some("d"));
        assert_eq!(o.paths, v(&["a", "b"]));
    }

    #[test]
    fn a_bad_format_is_an_error() {
        assert!(parse(&v(&["--format", "yaml"])).is_err());
    }

    #[test]
    fn budget_parses_with_a_unit() {
        let o = parse(&v(&["--budget", "15k"])).ok().unwrap_or_default();
        assert_eq!(o.budget, Some(15_000));
    }

    #[cfg(feature = "bpe")]
    #[test]
    fn bpe_flag_sets_the_tier() {
        let o = parse(&v(&["--bpe"])).ok().unwrap_or_default();
        assert_eq!(o.tier, Tier::Bpe);
    }

    #[test]
    fn a_bad_budget_is_an_error() {
        assert!(parse(&v(&["--budget", "xx"])).is_err());
    }

    #[test]
    fn window_parses_with_a_unit() {
        let o = parse(&v(&["--window", "1M"])).ok().unwrap_or_default();
        assert_eq!(o.window, Some(1_000_000));
    }

    #[test]
    fn model_takes_a_name() {
        let o = parse(&v(&["--model", "gpt-4o"])).ok().unwrap_or_default();
        assert_eq!(o.model.as_deref(), Some("gpt-4o"));
    }

    #[cfg(feature = "ollama")]
    #[test]
    fn ollama_flag_sets_the_tier() {
        let o = parse(&v(&["--ollama"])).ok().unwrap_or_default();
        assert_eq!(o.tier, Tier::Ollama);
    }

    /// B10, the regression: `--ollama <bare-host>` is the form the
    /// interface section documents (`[scheme://]host[:port]`, default port 11434) and the
    /// one the space heuristic must decline, because `box1` the host and
    /// `box1` the file are the same token. `=` binds it (V101).
    ///
    /// The bug was SILENT -- the bare IP was swallowed as a path and the
    /// tier fell back to localhost -- so this asserts both halves: the
    /// hosts arrive AND nothing leaked into the fileset.
    #[cfg(feature = "ollama")]
    #[test]
    fn an_equals_form_carries_a_bare_host() {
        let ip = crate::testutil::HOST_IP;
        let with_scheme = format!("http://{ip}");
        for host in [ip, crate::testutil::HOST_NAME, with_scheme.as_str()] {
            let o = parse(&v(&[&format!("--ollama={host}"), "SPEC.md"]))
                .ok()
                .unwrap_or_default();
            assert_eq!(o.tier, Tier::Ollama, "{host}");
            assert_eq!(o.ollama_hosts.as_deref(), Some(host), "{host}");
            assert_eq!(o.paths, vec!["SPEC.md"], "host leaked into paths");
        }
    }

    /// The `=` form is taken VERBATIM: no heuristic runs, so a value that
    /// the space form would have declined still reaches the tier.
    #[cfg(feature = "ollama")]
    #[test]
    fn the_equals_form_skips_the_heuristic_entirely() {
        let spaced =
            parse(&v(&["--ollama", "SPEC.md"])).ok().unwrap_or_default();
        assert_eq!(spaced.ollama_hosts, None, "declined: could be a path");
        let bound = parse(&v(&["--ollama=SPEC.md"])).ok().unwrap_or_default();
        assert_eq!(bound.ollama_hosts.as_deref(), Some("SPEC.md"));
    }

    /// An empty value ERRORS rather than resolving to localhost: the user
    /// named a value, and falling back silently is B10 in a new place.
    #[cfg(feature = "ollama")]
    #[test]
    fn an_empty_equals_value_is_an_error_not_a_default() {
        assert!(parse(&v(&["--ollama="])).is_err());
    }

    #[cfg(feature = "ollama")]
    #[test]
    fn ollama_consumes_a_host_list_but_not_a_path() {
        // A host-like token is the fleet (V24); a bare path stays a path.
        let o = parse(&v(&["--ollama", "box1,box2", "SPEC.md"]))
            .ok()
            .unwrap_or_default();
        assert_eq!(o.ollama_hosts.as_deref(), Some("box1,box2"));
        assert_eq!(o.paths, v(&["SPEC.md"]));

        let p = parse(&v(&["--ollama", "SPEC.md"])).ok().unwrap_or_default();
        assert_eq!(p.ollama_hosts, None);
        assert_eq!(p.paths, v(&["SPEC.md"]));
    }

    /// B11e/V71: a compiled-out flag is KNOWN, and the message names the
    /// FEATURE plus what to do. `unknown flag` sent the reader hunting for
    /// a typo while `--help`, printed directly below, advertised the flag.
    #[test]
    fn a_gated_flag_names_its_feature_not_a_typo() {
        for (flag, feature) in [
            ("--bpe", "bpe"),
            ("--ollama", "ollama"),
            ("--ollama=h", "ollama"),
        ] {
            let m = unknown(flag);
            assert!(m.contains(feature), "names the feature: {m}");
            assert!(!m.contains("unknown flag"), "not a typo: {m}");
            assert!(m.contains("--features"), "says what to do: {m}");
        }
    }

    /// A genuine typo still reads as one -- the gated path must not
    /// swallow every unrecognized flag.
    #[test]
    fn a_real_typo_still_says_unknown() {
        assert!(unknown("--noshuchthing").contains("unknown flag"));
        assert_eq!(gated("--nope"), None);
    }

    #[cfg(not(feature = "ollama"))]
    #[test]
    fn ollama_is_rejected_without_the_feature() {
        // V23: the offline build has no network tier -> unknown flag.
        assert!(parse(&v(&["--ollama"])).is_err());
    }

    #[test]
    fn top_needs_a_number() {
        assert!(parse(&v(&["--top"])).is_err());
        assert!(parse(&v(&["--top", "x"])).is_err());
    }

    fn tok() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("-s".to_owned()),
            Just("--top".to_owned()),
            Just("--format".to_owned()),
            Just("-C".to_owned()),
            Just("5".to_owned()),
            "[a-z0-9._/-]{0,5}",
            "-{1,2}[a-z]{1,3}",
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        /// Totality: no arg vector ever panics the parser.
        #[test]
        fn parse_never_panics(a in prop::collection::vec(tok(), 0..8)) {
            let _ = parse(&a);
        }

        /// Flag order does not change the parse (order-free parser).
        #[test]
        fn flag_order_is_irrelevant(
            tail in Just(vec!["-s", "-h", "x.rs"]).prop_shuffle()
        ) {
            let reference = parse(&v(&["-s", "-h", "x.rs"])).ok();
            prop_assert_eq!(parse(&v(&tail)).ok(), reference);
        }

        /// Any unknown `-flag` is always an error -- never ignored.
        #[test]
        fn unknown_flags_always_error(f in "-[q-z]{2,4}") {
            prop_assert!(parse(&[f]).is_err());
        }
    }
}
