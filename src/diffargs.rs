//! diff's argument parsing and small formatters, split from `diffcmd` at
//! the module-length limit (V284). `Raw` is the parsed request; `parse`
//! walks git diff's arg grammar into it (V7/V32).

use crate::args::Format;
use crate::units;

/// A parsed diff request, before git is consulted.
#[derive(Default)]
pub(crate) struct Raw {
    pub(crate) staged: bool,
    pub(crate) bpe: bool,
    pub(crate) exit_code: bool,
    pub(crate) budget: Option<u64>,
    pub(crate) format: Format,
    pub(crate) chdir: Option<String>,
    pub(crate) refs: Vec<String>,
    pub(crate) path: Option<String>,
}

pub(crate) fn parse(rest: &[String]) -> Result<Raw, String> {
    let mut r = Raw::default();
    let mut i = 0usize;
    let mut past = false;
    while let Some(a) = rest.get(i) {
        if past {
            r.path = Some(a.clone());
        } else if a == "--" {
            past = true;
        } else {
            flag(&mut r, a, rest, &mut i)?;
        }
        i = i.saturating_add(1);
    }
    Ok(r)
}

fn flag(
    r: &mut Raw,
    a: &str,
    rest: &[String],
    i: &mut usize,
) -> Result<(), String> {
    match a {
        "--staged" | "--cached" => r.staged = true,
        "--exit-code" => r.exit_code = true,
        #[cfg(feature = "bpe")]
        "--bpe" => r.bpe = true,
        "--budget" => r.budget = Some(units::parse(&val(rest, i)?)?),
        "--format" => r.format = fmt(&val(rest, i)?)?,
        "-C" => r.chdir = Some(val(rest, i)?),
        p if p.starts_with('-') => return Err(format!("unknown flag '{p}'")),
        p => r.refs.push(p.to_owned()),
    }
    Ok(())
}

fn val(rest: &[String], i: &mut usize) -> Result<String, String> {
    *i = i.saturating_add(1);
    rest.get(*i)
        .cloned()
        .ok_or_else(|| "flag needs a value".to_owned())
}

fn fmt(s: &str) -> Result<Format, String> {
    match s {
        "json" => Ok(Format::Json),
        "human" => Ok(Format::Human),
        other => Err(format!("unknown format '{other}'")),
    }
}

/// `+N` for a positive delta, plain for zero or negative.
pub(crate) fn signed(d: i64) -> String {
    if d > 0 {
        format!("+{d}")
    } else {
        d.to_string()
    }
}

pub(crate) fn breach_msg(added: u64, budget: u64) -> String {
    format!("itok: change adds {added} itok, over budget {budget}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn refs_and_path_separate_at_the_dashdash() {
        let r = parse(&v(&["A", "B", "--", "file.rs"]))
            .ok()
            .unwrap_or_default();
        assert_eq!(r.refs, v(&["A", "B"]));
        assert_eq!(r.path.as_deref(), Some("file.rs"));
    }

    #[test]
    fn budget_and_staged_parse() {
        let r = parse(&v(&["--staged", "--budget", "2k"]))
            .ok()
            .unwrap_or_default();
        assert!(r.staged);
        assert_eq!(r.budget, Some(2000));
    }

    #[test]
    fn a_bad_flag_errors() {
        assert!(parse(&v(&["--bogus"])).is_err());
    }

    #[test]
    fn format_human_parses() {
        let r = parse(&v(&["--format", "human"])).ok().unwrap_or_default();
        assert_eq!(r.format, Format::Human);
    }

    #[test]
    fn signed_marks_direction() {
        assert_eq!(signed(5), "+5");
        assert_eq!(signed(-3), "-3");
        assert_eq!(signed(0), "0");
    }
}
