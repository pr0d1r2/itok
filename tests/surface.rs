//! Section I promises a SURFACE, and the binary is what answers for it.
//!
//! B31: `itok doctor --session` was documented from the day the line was
//! written and answered `unknown flag` at exit 2. V106 is the rule for
//! exactly that, and the same section applies it correctly twice within
//! four lines -- `cap`'s ladder and `headroom --task` each carry a
//! "PLANNED, not BUILT" marker. doctor's line carried none. The marker is
//! applied by hand, so its own coverage was unenforced.
//!
//! Two directions, because only one of them has ever been looked for:
//!
//! FORWARD -- section I names a flag the build refuses. That is B31.
//! REVERSE -- the build advertises a flag section I never names. Nothing
//! has ever checked this, and it is how a surface grows undocumented.
//!
//! A traveling guard (V31/V13): itok cannot depend on a host crate, so a
//! rule the host enforces is one this repo enforces too.

use assert_cmd::Command;
use std::collections::{BTreeMap, BTreeSet};

const DIR: &str = env!("CARGO_MANIFEST_DIR");

/// Pairs section I names that the build does not offer, each with the
/// judgement that put it here. Hand-written on purpose: "is this line
/// promising a flag or describing one" is not a thing a parser decides,
/// and the marker in section I is the escape hatch V106 already defines.
///
/// A row saying PLANNED is a promise dated, not a promise broken.
const EXEMPT: &[(&str, &str, &str)] = &[
    (
        "cap",
        "--strip",
        "PLANNED, not BUILT (T40); the line says so",
    ),
    (
        "cap",
        "--dedup",
        "PLANNED, not BUILT (T40); the line says so",
    ),
    (
        "cap",
        "--elide",
        "PLANNED, not BUILT (T40); the line says so",
    ),
    (
        "cap",
        "--outline",
        "PLANNED, not BUILT (T40); the line says so",
    ),
    (
        "headroom",
        "--task",
        "PLANNED, not BUILT (T76); the line says so",
    ),
    (
        "top",
        "--cost",
        "PLANNED, not BUILT (T38); the line says so",
    ),
    // DESCRIBED, not offered. `check` pins the tokenizer rather than
    // taking a flag for it, and section I names `--bpe` to say the
    // verdict is deterministic -- a sentence about behaviour, not a
    // promise of a surface.
    ("check", "--bpe", "pinned, not selectable"),
];

/// Never probed: `--ollama` opens a network connection, and this suite
/// runs in a networkless sandbox by design (V34). Its absence here is a
/// gap this guard names rather than one it hides.
///
/// `--help` is not a flag itok has -- the binary prints usage for any
/// unknown one -- so where section I writes `--help` it is talking about
/// the OUTPUT, and probing it would test the sentence, not the surface.
const UNPROBED: &[&str] = &["--ollama", "--help"];

/// FORWARD: every flag section I promises, the build accepts.
#[test]
fn section_i_promises_no_flag_the_build_refuses() {
    let broken: Vec<String> = documented()
        .into_iter()
        .flat_map(|(verb, flags)| refused(verb, flags))
        .collect();
    assert!(
        broken.is_empty(),
        "section I promises what the binary refuses -- build it, or mark \
         it PLANNED as `cap` and `headroom` do (V106/B31):\n{broken:#?}"
    );
}

/// One verb's promised flags that the parser does not know.
fn refused(verb: String, flags: BTreeSet<String>) -> Vec<String> {
    flags
        .into_iter()
        .filter(|f| !skip(&verb, f) && refuses(&verb, f))
        .map(|f| format!("{verb} {f}"))
        .collect()
}

/// REVERSE: every flag the binary ADVERTISES, section I names.
///
/// The synopsis is what `--help` prints, so this asks the binary what it
/// claims and holds the spec to it. A flag can still be accepted and
/// advertised nowhere; that is the third direction, and probing for it
/// would mean guessing at names.
#[test]
fn the_advertised_surface_is_documented() {
    let spec = documented();
    let mut missing = Vec::new();
    for (verb, flags) in advertised() {
        let named = spec.get(&verb).cloned().unwrap_or_default();
        for flag in flags {
            if !named.contains(&flag) {
                missing.push(format!("{verb} {flag}"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "the binary advertises what section I never names:\n{missing:#?}"
    );
}

fn skip(verb: &str, flag: &str) -> bool {
    UNPROBED.contains(&flag)
        || feature_gated(flag)
        || EXEMPT.iter().any(|(v, f, _)| *v == verb && *f == flag)
}

/// Flags this BUILD does not have, as opposed to flags the project does
/// not have.
///
/// Section I documents the full surface; `--help` ends by naming the
/// tiers a given build carries, and `cargo nextest --no-default-features`
/// is a gate step precisely because that build must keep working. A
/// reduced build refusing `--bpe` is the feature gate doing its job
/// (V71: a gated flag names its feature), not a promise broken.
///
/// Read with `cfg!` rather than `#[cfg]` on the test, so the test still
/// RUNS in every build and reports what it probed. A guard that compiles
/// itself out under one feature set is a guard that passes because it was
/// not there (B32).
fn feature_gated(flag: &str) -> bool {
    (!cfg!(feature = "bpe") && flag == "--bpe")
        || (!cfg!(feature = "session") && flag == "--session")
}

/// Does the parser reject this flag outright?
///
/// "Unknown" plus the flag's own name is the answer that means "not
/// built". Matched that way rather than against one sentence, because the
/// verbs do not share one: `check` says `unknown argument` and everything
/// else says `unknown flag`, and both are right -- `check` takes no
/// positionals, so a bare word there is an argument, not a flag. A probe
/// pinned to one wording silently stops probing the other verb, which is
/// how this guard failed its own planted test the first time.
///
/// Any other failure -- a missing value, an absent file, a usage error
/// about something else -- proves the flag is KNOWN, which is all this
/// asks.
///
/// Run in an empty directory so a probe cannot do real work, with the
/// git environment cleared: git exports `GIT_DIR` into everything it
/// runs, and a probe reading the repo under test is B19.
fn refuses(verb: &str, flag: &str) -> bool {
    let Ok(mut probe) = Command::cargo_bin("itok") else {
        return false;
    };
    let out = probe
        .current_dir(sandbox())
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .args([verb, flag])
        .write_stdin("")
        .output();
    out.is_ok_and(|o| {
        let err = String::from_utf8_lossy(&o.stderr);
        err.contains("unknown") && err.contains(flag)
    })
}

/// An empty directory to probe from, so no probe can do real work: the
/// git-tracked default finds nothing and the dotfile registries are
/// absent, which is the fastest possible failure for every verb.
fn sandbox() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("itok-surface-probe");
    std::fs::create_dir_all(&dir).ok();
    dir
}

/// Verb -> the flags section I names for it.
///
/// Prose counts: a flag named anywhere in a verb's entry is a flag that
/// entry promises. But a bullet may name ANOTHER verb's flag while
/// explaining itself -- `headroom`'s says the verdict stays `doctor
/// --session` -- so a backtick span that opens with a verb name is
/// attributed to that verb, not to the bullet's.
fn documented() -> BTreeMap<String, BTreeSet<String>> {
    let path = std::path::Path::new(DIR).join("SPEC.md");
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let verbs: BTreeSet<String> = advertised().into_keys().collect();
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("- `itok ") {
            let owner = rest.split_whitespace().next().unwrap_or("");
            absorb(&mut out, &verbs, owner.trim_matches('`'), line);
        } else if line.starts_with("- `--") {
            shared_flag(&mut out, line);
        }
    }
    out
}

/// One bullet's spans, each attributed to whoever owns it.
fn absorb(
    out: &mut BTreeMap<String, BTreeSet<String>>,
    verbs: &BTreeSet<String>,
    owner: &str,
    line: &str,
) {
    for (i, span) in line.split('`').enumerate() {
        let inside = i % 2 == 1;
        let who = if inside {
            owner_of(verbs, span, owner)
        } else {
            owner
        };
        let mut found = long_flags(span);
        if inside && short_flag(span.trim()) {
            found.insert(span.trim().to_owned());
        }
        out.entry(who.to_owned()).or_default().extend(found);
    }
}

fn owner_of<'a>(
    verbs: &BTreeSet<String>,
    span: &'a str,
    fallback: &'a str,
) -> &'a str {
    let first = span.split_whitespace().next().unwrap_or("");
    if verbs.contains(first) {
        first
    } else {
        fallback
    }
}

/// A flag with a bullet of its own: `` - `--ollama[=HOSTS]` (estimate/doctor) ``.
/// Section I documents it once and names who takes it, so the guard reads
/// it the same way rather than demanding a copy in each verb's entry.
fn shared_flag(out: &mut BTreeMap<String, BTreeSet<String>>, line: &str) {
    let Some(flag) = long_flags(line).into_iter().next() else {
        return;
    };
    let Some(after) = line.split_once('(') else {
        return;
    };
    let Some((verbs, _)) = after.1.split_once(')') else {
        return;
    };
    for verb in verbs.split('/') {
        out.entry(verb.trim().to_owned())
            .or_default()
            .insert(flag.clone());
    }
}

/// Verb -> the flags its synopsis advertises, read from the one registry
/// `--help` renders (V40).
fn advertised() -> BTreeMap<String, BTreeSet<String>> {
    let path = std::path::Path::new(DIR).join("src/docs.rs");
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("synopsis: \"") else {
            continue;
        };
        let Some(verb) = rest.split_whitespace().next() else {
            continue;
        };
        out.entry(verb.to_owned()).or_default().extend(flags(rest));
    }
    out
}

/// Long flags anywhere in the text, and short ones only inside backticks
/// -- `-s` is a flag, "read-only" is not.
fn flags(line: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    out.extend(long_flags(line));
    for span in line.split('`').skip(1).step_by(2) {
        let name = span.trim();
        if short_flag(name) {
            out.insert(name.to_owned());
        }
    }
    out
}

fn long_flags(line: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for word in line.split(|c: char| !(c.is_alphanumeric() || c == '-')) {
        let name = word.trim_end_matches('-');
        if name.starts_with("--") && name.len() > 2 {
            out.insert(name.to_owned());
        }
    }
    out
}

fn short_flag(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next() == Some('-')
        && chars.next().is_some_and(char::is_alphabetic)
        && chars.next().is_none()
}
