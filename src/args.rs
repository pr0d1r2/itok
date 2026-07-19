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
    if a == "--ollama" {
        o.tier = Tier::Ollama;
        take_ollama_hosts(o, rest, i);
        return Ok(());
    }
    if boolean(o, a) {
        Ok(())
    } else {
        valued(o, a, rest, i)
    }
}

/// Consume the next token as `--ollama`'s host-list, but ONLY when it looks
/// like hosts -- a comma-list, `host:port`, or `-` (stdin) -- so a bare
/// path (`estimate --ollama SPEC.md`) stays a path, not a host (V24/V25).
#[cfg(feature = "ollama")]
fn take_ollama_hosts(o: &mut Opts, rest: &[String], i: &mut usize) {
    if let Some(next) = rest.get(i.saturating_add(1)) {
        if next == "-" || next.contains(',') || next.contains(':') {
            o.ollama_hosts = Some(next.clone());
            *i = i.saturating_add(1);
        }
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
        p if p.starts_with('-') => return Err(format!("unknown flag '{p}'")),
        p => o.paths.push(p.to_owned()),
    }
    Ok(())
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
