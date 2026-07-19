//! Real-tokenizer tier (V4): `o200k_base` via tiktoken-rs. The ranks are
//! embedded in the crate, so this is offline at runtime and deterministic
//! (V12, V5); only the build-time crate fetch touches the network, and
//! the flake vendors that. Behind the `bpe` feature, so
//! `--no-default-features` yields the zero-dep dummy-only tool (V13).

use std::sync::OnceLock;
use tiktoken_rs::CoreBPE;

/// The o200k encoder, built once. Rebuilding it per file would reload the
/// ranks each time; a token counter should be snappy.
fn encoder() -> Option<&'static CoreBPE> {
    static ENC: OnceLock<Option<CoreBPE>> = OnceLock::new();
    ENC.get_or_init(|| tiktoken_rs::o200k_base().ok()).as_ref()
}

/// Count o200k tokens in `text`: a real tokenizer's TRUE count, so no
/// tilde and `estimated:false` (V3). `encode_ordinary` skips special-token
/// framing -- we count content, not chat structure.
#[must_use]
pub fn count(text: &str) -> u64 {
    encoder().map_or(0, |e| {
        u64::try_from(e.encode_ordinary(text).len()).unwrap_or(u64::MAX)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_zero() {
        assert_eq!(count(""), 0);
    }

    #[test]
    fn hello_world_is_two_tokens() {
        // o200k splits "hello world" into "hello" + " world".
        assert_eq!(count("hello world"), 2);
    }

    #[test]
    fn it_is_deterministic() {
        assert_eq!(count("the quick brown fox"), count("the quick brown fox"));
    }

    #[test]
    fn more_text_is_more_tokens() {
        assert!(count("a b c d e f g h") > count("a"));
    }
}
