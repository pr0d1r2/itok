# Changelog

All notable changes to `itok` are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com), and this project adheres to
[Semantic Versioning](https://semver.org).

## Versioning

Each minor version is a level of **guarantee**, not a feature count -- what
you can rely on at that tag:

| version | guarantee |
|---------|-----------|
| `0.1.0` | builds reproducibly on any machine |
| `0.2.0` | cannot regress locally -- the gate runs in git hooks |
| `0.4.0` | knows what a context costs (runtime telemetry) |
| `0.6.0` | can act on it (reduction, policy, fuse) |
| `0.7.0` | featureset frozen |
| `0.8.0` | public gate trustworthy |
| `1.0.0` | contract frozen -- CLI surface and JSON output |

Odd minors (`0.3`, `0.5`, `0.9`) are reserved for fix releases between
milestones. Before `1.0.0` a minor bump may change behaviour: each rung is
a behaviour change, and saying so is the honest reading of SemVer 0.x.

**Publication is not a rung.** This table used to mark `0.7.0` as the first
public release, which welded *who can install it* onto *what it guarantees*.
Those move independently, and fusing them left only two options: wait, or
overstate maturity in order to ship. So `itok` is public from `0.3.0-rc.1`,
at the rung that is actually true -- `0.4.0`'s telemetry is still open, and
the version number says so.

## [0.3.0-rc.1] - 2026-08-08

**First public release.** An odd minor, which is a fix release between
milestones -- no milestone moved. Published early on purpose: the tool is
operable, and real use finds defects that another pass over our own tree
does not. The last two here were found by a user and by a reader, not by the
gate.

### Security

- **`--ollama` now supports TLS.** `ureq 3` with `rustls`, so
  `--ollama=https://host` works. This matters more than it sounds: an exact
  count means handing the file's text to the tokenizer, and the tokenizer is
  on the other host -- so the exact tier transmits **file contents**, over
  `http://` in cleartext, to a host taken from `OLLAMA_HOST`,
  `.context-hosts` or the command line. Nothing constrained that to a LAN.
- Fixed a promise the build could not keep: `https://` was accepted by the
  parser and preserved in the method label, while TLS was compiled out --
  so it opened a TCP session and *then* failed. No test covered the path,
  because the cassette replay serves plain HTTP.
- `docs/SECURITY.md` corrected. It had listed "no TLS stack" as a hardening
  property beside `forbid(unsafe_code)` -- a missing mitigation in the
  strengths column -- and declared plaintext transport out of scope for
  reports.
- `http://` remains the default and is still supported; a LAN box you own
  does not want a certificate.

### Added

- Public documentation: `docs/SECURITY.md`, `docs/CODE_OF_CONDUCT.md`,
  `docs/THIRD-PARTY-NOTICES.md`, `docs/INTEGRATION.md`, and
  `CONTRIBUTING.md` moved in beside them.
- Generated README badges -- nineteen, every number read from the file that
  owns it, so a badge cannot drift from the manifest, the flake or the
  coverage floor.
- Eight gate steps: `ripsecrets`, `semver`, `links`, `smart-quotes`,
  `readme-badges`, `integration-doc`, plus `package` now checking the
  tarball's *contents* rather than only that one can be built.
- `hk run refresh` to update the coverage cache and the badges.

### Changed

- MSRV `1.82` -> `1.96`, matched to the toolchain the gate actually runs.
  The old number was never compiled against.
- The `SPEC.md` format checker is a pinned flake input (`microlith 0.5.0`)
  rather than a sibling checkout, so a fresh clone can build and commit.
- The `ollama` tier went from 60 dependencies to **44**, and its 19
  `Unicode-3.0` licences to zero. Adding TLS made it smaller: `ureq 2`
  pulled `url` -> `idna` -> the whole ICU stack.

### Fixed

- `tests/ci.rs` shipped in the `.crate` while reading `hk.pkl`, which is
  excluded -- three of its eight tests failed on the published tarball while
  staying green in the repo. Found by measuring the tarball, not by the
  suite.
- README documented a `crates/itok` install path that cannot exist in a
  clone, and an example output with numbers stale by half.
- Removed hardcoded LAN addresses from help text, tests and the spec.

## [0.2.0] - 2026-07-25

The gate cannot be bypassed by forgetting. Not a public release --
`0.7.0` is.

### Added

- One gate definition in `hk.pkl`, run by [hk], reached three ways from
  that single file: the `pre-commit` hook, the `pre-push` hook, and
  `hk check` in CI. The ops cannot drift, because there is one copy.
- Nix packages: `nix build` / `nix run` with no Rust toolchain.
  `default` (with `--bpe`), `itok-minimal` (the zero-dependency core),
  `itok-ollama` (the LAN-exact tier).
- The dev shell provides `itok` itself, so the tool is on `PATH` while
  you work on it.

### Fixed

- Cassette replay stub read the request with a single `read()`. TCP is a
  stream, so the one POST could have its body arrive in a second segment,
  and the stub replied and closed the socket mid-write. Flaky in 2 of 3
  parallel runs; green in 8 of 8 after the fix.

### Removed

- `.file-limits`. No standalone runner ever read it, so its ceilings were
  maintained by hand and enforced by nothing. File-size limits remain a
  guard in the repository where itok is developed.

## [0.1.0] - 2026-07-25

The command surface, and a build that reproduces anywhere. Not a public
release.

### Added

- `estimate` -- token cost of files, git-tracked by default, `du`-shaped.
- `diff` / `show` / `log` -- token cost of changes across git history,
  `git`-shaped.
- `check` -- gate registered paths against a committed `.context-limits`.
- `doctor` -- advisory fit-to-window / balance / noise / confidence.
- `fit` -- greedy subset of files under a `--window` budget.
- `docs` -- print this command reference as markdown (the README block).
- Precision ladder: `dummy` (bytes/4), `--bpe` (o200k, offline,
  deterministic), `--ollama` (a local model's own tokenizer over the LAN).
- `--budget N` inline gate, `--format json` stable output, and prefix
  inference on every verb.

[hk]: https://hk.jdx.dev
[0.3.0-rc.1]: https://github.com/pr0d1r2/itok/releases/tag/v0.3.0-rc.1
[0.2.0]: https://github.com/pr0d1r2/itok/releases/tag/v0.2.0
[0.1.0]: https://github.com/pr0d1r2/itok/releases/tag/v0.1.0
