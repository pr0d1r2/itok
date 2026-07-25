# Changelog

All notable changes to `itok` are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com), and this project adheres to
[Semantic Versioning](https://semver.org).

## [0.8.0] - 2026-07-25

First public release. Stable and battle-tested, but pre-1.0: the command
surface and output formats may still change before 1.0.

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

[0.8.0]: https://github.com/pr0d1r2/itok/releases/tag/v0.8.0
