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
- **Zero dependencies at the minimal tier.** `--no-default-features` has no
  third-party runtime code at all — see
  [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md).

**TLS is available, and this document used to get that backwards.** An
earlier version of this file listed "no TLS stack" in the list above, as
though it belonged beside `forbid(unsafe_code)`. It does not. Those entries
remove a *class of vulnerability*; an absent TLS stack removes a
*mitigation*, and calling that hardening is a category error. It also listed
plaintext transport as out of scope, which declared a real weakness
unreportable.

`ureq` is now taken with `rustls`, so `--ollama=https://host` works. Use it
whenever the endpoint is not something you would be comfortable shouting
across the network, because of what the exact tier actually sends:

> **`--ollama` transmits the contents of your files.** Getting an exact
> count means handing the text to a tokenizer, and the tokenizer is on the
> other host — `itok` POSTs the file body to `/api/generate`. Over `http://`
> that is cleartext. The host comes from `OLLAMA_HOST`, `.context-hosts` or
> the command line, and nothing constrains it to a LAN.

`http://` remains the default and remains supported. A LAN endpoint you
control is a reasonable thing to want, and forcing TLS onto it would mean
certificates for a box on your own desk. But the choice is yours to make
knowingly, which is what this section is for.

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
4. **The network backend.** `ollama` sends file contents to a host you name,
   over `http://` unless you ask for `https://`. Worth reporting: a path
   where the *default* build reaches the network at all; a response that can
   do more than produce a number; an `https://` endpoint whose certificate is
   not actually verified; or any case where a scheme you asked for is
   silently downgraded.
5. **A count that is silently wrong.** `itok`'s entire premise is that a number
   names its own method and marks itself as an estimate. A path that reports a
   confident-looking figure it did not actually measure is treated as a defect
   of the same seriousness as a crash.

## What is out of scope

- Reading a file you pointed it at. `itok estimate <path>` reads that path;
  that is the tool working.
- Choosing `http://` for a LAN endpoint and getting plaintext. That is the
  documented behaviour of the scheme you asked for, and `https://` is
  available. Reporting that `http://` is unencrypted is reporting HTTP.
- Disagreeing with an estimate's accuracy. The tier ladder and its error bands
  are in [`SPEC.md`](../SPEC.md); an inaccurate-but-honest number is an
  ordinary issue.
- Anything requiring an attacker who already runs code as you. They do not need
  a token estimator.
