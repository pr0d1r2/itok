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

**A pre-release is not a rung either.** `0.3.0-rc.1` and `0.3.0` claim the
same guarantee; the suffix said only that the publishing pipeline was
unproven. It is proven now, so the suffix is gone and the ladder is
unchanged.

## [0.3.1] - 2026-08-22

**A badge that reports itself.** `itok rate` turns a session's throughput
into one pre-formatted string a statusline hook can print, so context spend
is visible while it happens instead of in a postmortem. The guarantee level
is unchanged -- this is additive and report-only, which is what the `0.3.x`
band is for; `0.4.0`'s telemetry rung stays open.

### Added

- **`itok rate [<session>]`** -- `last_turn,total/total,rate/h,rate/d`, each
  value shortened with ceiling rounding so 900 tokens reads `1k` and never
  `0k`. `--format json` returns the same numbers unrounded, plus `turns`,
  `age_seconds` and `active_seconds`. 0-1 turns emits nothing, so a fresh
  session shows no badge rather than a zero one.
- **`itok rate --statusline`** -- reads the harness statusline payload on
  stdin and takes `transcript_path` and `cwd` from it, then emits the
  wrapped badge `(itok:...)`. The badge therefore names the session it is
  drawn beside; inferring the newest transcript in the directory showed a
  concurrent session's numbers whenever more than one was open on the same
  repo. The mapping lives in `hook.rs` with every other harness-shaped
  thing, so no `jq` wrapper and no new dependency. Stdin is touched only
  under the flag -- bare `rate` never blocks on a terminal. Colour defaults
  to `always` here, because the harness captures stdout and leaves no tty to
  detect.
- **`itok.toml`** -- optional `[rate]` thresholds driving per-metric ANSI
  colour (green/amber/red, decided independently per value). A metric with
  no threshold gets no colour opinion, so no config file means plain text
  rather than an accent nobody asked for.
- **Projection is marked, not implied.** `/h` and `/d` carry `~` when the
  sample does not cover the period they name -- an hour-rate off twenty
  minutes is an extrapolation. json carries `projected_hour` and
  `projected_day` booleans instead, because a tilde does not belong inside
  a number.

### Changed

- **Rates divide by working time, not by the calendar.** The denominator is
  the sum of inter-turn gaps with each gap credited at most 300 seconds,
  rather than the wall-clock span from first turn to last. A session left
  open overnight was reporting the sleep as work: the badge that prompted
  this read `3m/h` and reads `57m/h` measured the same way. `age_seconds`
  stays in the json beside `active_seconds` so both clocks remain visible.
- **Rust edition 2024**, with the MSRV unchanged at 1.95. The two halves
  fit: nixos 26.05 carries rustc 1.95.0 and edition 2024 needs 1.85, so the
  MSRV is the binding constraint and the edition costs nothing in reach.
  Standardised across the fleet.
- **`resolve_session` takes its ambient inputs as arguments.** Edition 2024
  makes `std::env::set_var` `unsafe`, and `unsafe_code = "forbid"` is not
  negotiable here, so the two tests that set `HOME` (and put it back) now
  hand the planted tree to a `resolve_with` seam instead. The lint asked the
  right question: a process-wide mutation made to answer a question about
  one call is unsound under a parallel runner in any edition.

### Fixed

- **`rate` read transcript timestamps as epoch digits** (`§B24`). Every
  transcript writes RFC 3339 (`2026-08-15T06:55:39.102Z`), so the parse
  failed silently and `age_seconds` was 0 for every real session -- which
  the one-second floor turned into `132590m/h` on a 22-hour session. The
  reader is now a fixed-offset RFC 3339 parser pinned against values
  computed outside the crate, and the fixtures were re-cut into the shape
  the producer actually emits. Twenty-five green tests missed it because
  the fixture stamps had been hand-written to match the parser: the suite
  proved the code agreed with itself.

## [0.3.0] - 2026-08-14

**The rc's number, spent.** Everything below under `0.3.0-rc.1` ships here
unchanged in intent; what changed is that the pipeline it existed to prove
has now been proven, and the defects it surfaced have runners.

The pre-release earned its keep by failing in five places no dry run
reached:

| | what it was |
|---|---|
| `§B17` | fifteen tests failed on the published `.crate` — `--verify` only *compiles* it |
| `§B18` | `itok show <merge>` reported zero; `git diff-tree` answers empty for merges and root commits |
| `§B19` | `-C` is not isolation — every verb could answer about whichever repo invoked it |
| `§B20` | a coverage badge no single machine could reproduce |
| `§B21` | the gate hanging for 74 minutes instead of reporting |

`§B17` and `§B18` would have been **permanent** in a non-pre-release
version, which is the entire argument for having spent an rc.

### Changed

- The dev shell, the toolchain and the gate all moved under the rc — nixos
  26.05 via the fleet lock, rustc 1.95.0, hk from `nix-hk`, microlith 0.6.1.
  Those entries are recorded under `0.3.0-rc.1` below, because that is the
  version they were developed against.
- **The release runs through `cargo-release`**, configured by `release.toml`.
  `0.3.0-rc.1` was published by hand from prose in `§T59` — eight commands
  whose order was remembered rather than enforced. The gate is now a
  `pre-release-hook`, so publishing is conditional on all 33 steps rather
  than on the releaser having run them.

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
- Nine gate steps: `ripsecrets`, `semver`, `links`, `smart-quotes`,
  `readme-badges`, `integration-doc`, plus `package` now checking the
  tarball's *contents* rather than only that one can be built, and
  `package-suite`, which unpacks the `.crate` outside the worktree and runs
  its own tests the way a consumer would.
- `hk run refresh` to update the coverage cache and the badges.
- **The release is configured, not scripted.** `release.toml` drives
  `cargo-release`; there is no release script and there should not be one.
  `0.3.0-rc.1` was published by hand from prose in `§T59` — eight commands
  whose *order* was remembered rather than enforced, which is a rule with no
  runner (`§V17`).

  The version bump still goes through a pull request, because `main` requires
  one with twelve passing checks. Only the tail runs from `main` afterwards,
  and `cargo release hook` must be first: `tag`, `publish` and `push` do
  **not** run `pre-release-hook`, so a sequence starting at `tag` would
  publish whatever the tree happened to hold.

  The hook is `hk check --all --check --no-fail-fast`, which makes the whole
  gate a precondition of publishing rather than something the releaser is
  trusted to have run — including `package-suite`, the step that catches what
  `cargo publish --verify` cannot, since verify only *compiles* the tarball
  (`§B17`).
- CI caches the nix store and the cargo target directory, runs the three
  `nix build`s as a concurrent job, and carries a `timeout-minutes` bound.

  The caches are a modest win and this entry says so, because the first
  draft of it did not. CI was never slow: measured cold, the dev shell
  materialises in **26 seconds** and the gate reaches its first test in
  **79**. The hour-long runs were a hang, not a cost.

  The bound is the part that mattered, and it was added as a diagnostic
  rather than hygiene: GitHub serves no logs for an in-progress job and
  defaults to 360 minutes, so a hang was six hours of silence with nothing
  to read. It fired at 45m16s and produced the log that found the hang.
- **CI no longer passes `--no-fail-fast`.** hk stops producing output
  entirely once a `depends`-chained step fails under that flag — measured
  twice, 74 and 43 minutes of complete silence, both ended by a kill rather
  than a verdict. A complete failure list is worth nothing from a run that
  never reports one, so CI now returns its first failure in about ninety
  seconds. Restore the flag when hk no longer deadlocks.
- **CI runs on every system the flake declares** — `x86_64-linux`,
  `aarch64-linux` and `aarch64-darwin`, one runner each, with all three
  feature configurations built on all three. Twelve concurrent jobs. A
  matrix replicates the gate rather than dividing it, so wall clock stays
  roughly flat while compute triples; what it buys is that the platform
  claim is gated instead of asserted. This project has already shipped two
  defects invisible from a single vantage point.

  **No MSRV axis**, and that is a decision rather than a gap. Such a job can
  only fail when the declared minimum sits below the toolchain CI runs, and
  here they are the same number — `rust-version` and the `flake.lock` pin are
  kept equal by policy, so the axis would recompile the identical toolchain
  for an identical answer. It becomes worth having the day the declared
  minimum drops below the pin.
- CI actions are pinned to commit SHAs rather than tags, with the tag kept
  in a comment. A tag is mutable, so trusting one hands whoever controls
  the action a push into this repository's CI. `workflow_dispatch` too --
  both propagated from `microlith`, which already made these choices.

### Changed

- MSRV `1.82` -> `1.96` -> **`1.95`**, matched to the toolchain the gate
  actually runs. `1.82` was never compiled against; `1.96` became `1.95` when
  the dev shell moved to the fleet's nixos 26.05 lock, which carries rustc
  1.95.0. Lowering an MSRV widens who can build the crate, so it is
  compatible in the direction that matters.
- **The dev shell moves to nixos 26.05**, via `nixpkgs-lock` as the single
  nixpkgs authority rather than a rev pinned here. One rev across every repo
  is what lets the shared binary cache hit instead of rebuilding.

  26.05 dropped `pkgs.hk` entirely — the shell stopped evaluating with
  `attribute 'hk' missing`, which is the whole gate gone in one bump. hk now
  comes from its own flake (`nix-hk`), which makes the runner of every rule
  here an explicit, versioned choice instead of a side effect of whatever
  nixpkgs happens to carry.

  That also raised hk 1.51.0 -> **1.55.0**, which fixes the deadlock recorded
  as `§B21`. Verified against the same minimal fixture that hung three times
  out of three: a dependent of a failed step now runs to completion instead
  of waiting forever.
- **`--no-fail-fast` returns, for pushes to `main` only.** A pull request's
  merge is blocked by a red check either way, so the first failure in ninety
  seconds beats a complete list in ten minutes. On `main` nobody is waiting,
  so every failure is reported at once.
- `microlith` `0.5.0` -> `0.6.1`. Both earlier tags declare
  `rust-version = "1.96"` and could not build on 1.95.0, so the old pin made
  the shell unbuildable on 26.05.
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
- Fifteen tests failed on the published tarball -- the same class as the
  line above, one layer deeper. They read *itok's own git history*, and
  registry source has no `.git`, so anyone vendoring, auditing or packaging
  the crate would have run `cargo test` and got fifteen reds. Nothing here
  could see it: `cargo package --verify` only **compiles** the tarball, and
  the `package` step checks which files ship, not that they work. They are
  now gated on `ITOK_DOGFOOD`, which this repo's gate and dev shell set --
  an explicit opt-in rather than a "am I in a repo?" probe, which would have
  gone green here for the wrong reason and still fired for a vendorer inside
  their own repo. `package-suite` is the runner that would have caught it.
- README documented a `crates/itok` install path that cannot exist in a
  clone, and an example output with numbers stale by half.
- **`itok show <merge-commit>` reported zero.** `git diff-tree` answers
  empty for a merge -- it declines to pick a side -- and for the root
  commit, where there is no side. Both came back as exit 0 with no
  diagnostic, so the tool gave a confident zero-cost answer for a commit
  that changed plenty. Merges now read against their first parent, which is
  the commit `show` was already computing its per-file deltas against; the
  root commit uses `diff-tree --root`.

  Worth saying why it shipped: this repository's history is linear, and
  branch protection now requires it to stay that way, so the commit shape
  the tool most needs to handle is the one its own repo can never contain.
  It surfaced within a minute of the first pull request ever opened here,
  because GitHub checks out an ephemeral merge commit for `pull_request`.
- **Every verb could answer about the wrong repository.** `git -C <root>`
  sets the working directory; `GIT_DIR`, `GIT_WORK_TREE` and `GIT_INDEX_FILE`
  set the *git* directory, and the environment wins. Git exports those into
  anything it runs, and `itok` runs from hooks by design -- this repo's own
  gate calls `itok check` from `pre-commit`, and `guard` is a harness hook.
  So `itok -C /other/repo show HEAD`, invoked from a hook, read the repo that
  invoked it and said nothing about the substitution. It looked correct here
  only because the two paths coincide.

  Every git call in `gitref`, `walk` and the test helpers now goes through
  one constructor that clears the nine `GIT_*` variables. Verified by running
  the entire suite with a decoy `GIT_DIR` set: 426 of 426, with it and
  without.
- **A test that checked nothing on half the machines it ran on.**
  `a_stale_harness_id_falls_back_instead_of_failing` asserted inside an
  `if let Ok(..)` against the ambient home and working directory, so on a
  machine with no transcript directory for the current cwd it took the
  `Err` path, verified nothing, and still reported green.

  The two halves of that turned out to be one defect. Because the branch ran
  on Linux and not on macOS, the suite's line coverage differed by exactly
  one line between platforms — 98.01% against 97.99% — with the 98% floor
  sitting precisely between them. `main` was simultaneously green on Linux
  and red on macOS, and nothing could see it because CI was Linux-only. It
  surfaced while adding the platform matrix, before that change merged.

  The test now plants its own transcript under a throwaway `HOME`, sets a
  stale `ITOK_SESSION_ID`, and asserts unconditionally that resolution falls
  back to newest-by-mtime. Coverage is 98.02% on both platforms — the same
  number, which is the point.
- **The coverage badge printed a number no machine could reproduce.** Line
  coverage is platform-dependent -- the identical source measures `98.04` on
  macOS and `98.06` on ubuntu -- and the gate compared cached against
  measured as exact strings. That cannot pass on both at once: the CI number
  fails your `pre-push`, and your number fails CI. The badge and the cache
  now carry one decimal, **truncated**. Truncated rather than rounded because
  `98.04` and `98.06` straddle `.05`, so rounding would have sent them to
  `98.0` and `98.1` and changed nothing; truncation sends both to `98.0` and
  floors, so the badge understates rather than overstates. The
  `--fail-under-lines 98` floor keeps full precision -- it is a threshold,
  not a published claim.
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
