//! Token-count units, decimal (V18): `1M` = 1_000_000, `200k` = 200_000.
//! One grammar shared by `--budget` (T12) and `--window` (T14), so there
//! is a single unit thing to learn (V489: one unit grammar, less to
//! hold). Multiplication is checked -- overflow is an error, not a panic.

/// Parse a token count: a plain integer, or a `k`/`M` suffix (decimal).
pub(crate) fn parse(s: &str) -> Result<u64, String> {
    let (digits, mult) = split(s);
    digits
        .parse::<u64>()
        .ok()
        .and_then(|n| n.checked_mul(mult))
        .ok_or_else(|| format!("bad count '{s}'"))
}

fn split(s: &str) -> (&str, u64) {
    if let Some(n) = s.strip_suffix('M') {
        (n, 1_000_000)
    } else if let Some(n) = s.strip_suffix('k').or_else(|| s.strip_suffix('K'))
    {
        (n, 1_000)
    } else {
        (s, 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_integer() {
        assert_eq!(parse("200000"), Ok(200_000));
    }

    #[test]
    fn k_is_a_thousand() {
        assert_eq!(parse("15k"), Ok(15_000));
        assert_eq!(parse("2K"), Ok(2_000));
    }

    #[test]
    fn m_is_a_million() {
        assert_eq!(parse("1M"), Ok(1_000_000));
    }

    #[test]
    fn junk_is_an_error() {
        assert!(parse("abc").is_err());
        assert!(parse("1.5k").is_err());
        assert!(parse("").is_err());
    }
}
