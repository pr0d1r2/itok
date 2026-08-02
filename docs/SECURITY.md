# Security policy

## Reporting a vulnerability

Report privately, not in a public issue.

- Preferred: [GitHub private vulnerability
  reporting](https://github.com/pr0d1r2/itok/security/advisories/new)
- Or email **pr0d1r2@gmail.com** with `itok security` in the subject.

Please include what you ran, what happened, and the input that triggered it. A
reproducing file is worth more than a description of one — but see the warning
below before you send a transcript.

> **Do not attach a real session transcript.** They contain whatever you
> discussed with a model: file contents, message text, tokens you pasted. If
> the bug is in transcript parsing, send the smallest *synthetic* `.jsonl` that
> reproduces it, or describe the shape and we will construct one.

Expect an acknowledgement within a week. If a report is valid, the fix and the
advisory go out together, and you are credited unless you ask otherwise.

## Supported versions

Pre-1.0, only the latest published version is supported. There are no backports
to earlier minors — see [`CHANGELOG.md`](../CHANGELOG.md) for what each rung of
the version ladder means.

## What the attack surface actually is

Stated plainly, because it is unevenly distributed and that changes what is
worth reporting.

**What is structurally excluded:**

- **No `unsafe`.** `unsafe_code = "forbid"` in `Cargo.toml`, so it cannot be
  reintroduced in a local module. Memory-safety bugs are not representable.
- **No panics by policy.** `unwrap_used`, `expect_used`, `panic`,
  `indexing_slicing` and `arithmetic_side_effects` are all `deny` at the lint
  level, so an abort on hostile input is a defect rather than a design.
- **No TLS stack.** The `ollama` feature takes `ureq` with default features
  off. There is no `ring`, no `openssl`, no certificate handling.
- **Zero dependencies at the minimal tier.** `--no-default-features` has no
  third-party runtime code at all — see
  [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md).

**What the surface genuinely is, in rough order of interest:**

1. **Transcript parsing.** With the `session` feature, `itok` reads harness
   `.jsonl` transcripts — a *foreign, unversioned, third-party* schema that
   nobody here controls. This is the largest untrusted-input surface in the
   crate, and the one where a hostile or merely malformed file is most likely
   to find something. Report anything here.
2. **Content disclosure.** Transcripts hold real conversation content. Any path
   where `itok` writes transcript *content* — rather than counts derived from
   it — into output, a ledger, an error message or a JSON field is a
   confidentiality bug, not a cosmetic one. `SPEC.md` `§V45` exists because
   this content must never end up committed.
3. **Subprocess execution.** `itok` shells out to `git` (`src/gitref.rs`,
   `src/walk.rs`) for tracked files and for reading blobs at a revision.
   Anything that lets a file name, a revision string or a pathspec influence
   the argument vector in an unintended way is worth reporting.
4. **The network backend.** `ollama` speaks plain HTTP to a host you name. It
   is unauthenticated and unencrypted by design — that is a LAN assumption, and
   is stated rather than hidden. Report a path where the *default* build
   reaches the network, or where a response can do more than produce a number.
5. **A count that is silently wrong.** `itok`'s entire premise is that a number
   names its own method and marks itself as an estimate. A path that reports a
   confident-looking figure it did not actually measure is treated as a defect
   of the same seriousness as a crash.

## What is out of scope

- Reading a file you pointed it at. `itok estimate <path>` reads that path;
  that is the tool working.
- The `ollama` backend being plaintext. It is documented, feature-gated, off by
  default, and deliberate.
- Disagreeing with an estimate's accuracy. The tier ladder and its error bands
  are in [`SPEC.md`](../SPEC.md); an inaccurate-but-honest number is an
  ordinary issue.
- Anything requiring an attacker who already runs code as you. They do not need
  a token estimator.
