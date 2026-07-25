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
| `0.7.0` | **first public release**; featureset frozen, CI unproven |
| `0.8.0` | public gate trustworthy |
| `1.0.0` | contract frozen -- CLI surface and JSON output |

Odd minors (`0.3`, `0.5`, `0.9`) are reserved for fix releases between
milestones. Before `1.0.0` a minor bump may change behaviour: each rung is
a behaviour change, and saying so is the honest reading of SemVer 0.x.

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
[0.2.0]: https://github.com/pr0d1r2/itok/releases/tag/v0.2.0
[0.1.0]: https://github.com/pr0d1r2/itok/releases/tag/v0.1.0
