# Fixing a failed gate

For agents and humans. The gate is silent when it passes; if you are
reading this, something failed.

## The rule

**Never bypass.** `--no-verify`, `HK=0`, lowering a threshold, deleting a
test, or adding `#[allow]` to silence clippy are all ways of shipping the
defect with the alarm switched off. Fix the cause. If the check itself is
wrong, that is a spec change: say so in `SPEC.md` and change the law
deliberately, in its own commit.

## Reproduce

```bash
hk check --all --check              # the whole gate, as CI runs it
hk check --all --check -S clippy    # one step
```

Every step is a plain cargo command. If `hk` is not installed, read the
command out of `hk.pkl` and run it directly -- the gate never depends on
`hk` to be reproducible.

One step needs a binary that is not cargo: `mth`, the binary of the
`microlith` package, which owns this spec's format and is the only thing
checking `SPEC.md`'s structure. Package and binary are named differently
on purpose -- `microlith` is what you depend on, `mth` is what you run.
The dev shell provides it from a flake input pinned to a released tag, so
`direnv allow` or `nix develop` is enough and nothing has to sit beside
this repo. It fails hard rather than skipping when absent -- a skipped
structural check reads exactly like a passing one, and this repo has
already paid for that once (`B12`).

To try an untagged format change against this spec, point
`MICROLITH_MANIFEST` at a sibling checkout's `Cargo.toml` and `mth` runs
that instead. It is opt-in on purpose: a sibling that outranked the pin
just by existing would make the gate's verdict depend on the layout of
the machine it ran on.

## What each failure means

| step | meaning | fix |
|------|---------|-----|
| `fmt` | formatting drift | `cargo fmt` |
| `clippy` | deny-list hit: `unwrap`, `panic`, indexing, silent overflow, or a function/complexity cap | Fix the cause. The caps are a design tool: a function that will not fit usually wants splitting, not a bigger cap. |
| `test` | a test failed | Read the assertion. If it is a race, fix the race -- do not serialise the suite (see `B5`). |
| `doctest` | a doc example failed | Doc examples are compiled. Mark non-code blocks ` ```text `. |
| `itok` | a registered path exceeded its `.context-limits` ceiling | Compact the file. Raising the ceiling is a reviewed decision, not the default move. |
| `mth` | `SPEC.md` is not formatted: a hard-wrapped statement, or a line over the cap | `mth fmt SPEC.md` joins the wraps. An over-long line is not fixed by the formatter -- split the statement. Raising the cap is a reviewed decision. |
| `mth-check` | `SPEC.md` broke a structural rule | Each line names the rule, the line, why it matters, and ranked fixes. A `mechanical` direction is deterministic and safe to apply unattended; a `judgment` one accepts a regression or changes intent, so stop and decide rather than applying it to make the gate green. |
| `ollama` | the cassette-replayed network path broke | The cassette is a *recording*: do not edit `tests/fixtures/` to make a test pass. Re-record deliberately if the protocol really changed. |
| `no-default-features` | the zero-dependency core stopped building | Something outside a `cfg` gate reached for an optional dependency. Feature-gate it. |
| `package` | the published `.crate` would contain the wrong files, or would be MISSING a required one | An extra file is usually an untracked-but-unignored path -- add it to `.gitignore` or `exclude`. A missing one is named by path: it is on `must-package` because a shipped test reads it, so fix `exclude`, or drop it from the list and say why it is no longer needed. This failure cannot reproduce by running the suite here -- the repo still has the file. |
| `rustdoc` | a doc comment breaks docs.rs | Usually `<angle>` text read as an HTML tag. Fence it as ` ```text `. |
| `ripsecrets` | a possible secret is staged | If it is real, it must never be committed -- a token in a public git history outlives its deletion. If it is a false positive, add it to `.secretsignore`. Note this reads the files in front of it, so it says nothing about history. |
| `readme-badges` | the README badge block no longer matches its sources | `hk fix` regenerates it. Never hand-edit the block -- every number is read from the file that owns it (`Cargo.toml`, `flake.nix`, `flake.lock`, `hk.pkl`, `.coverage`). If the coverage number is what moved, `hk run refresh`. |
| `links` | a relative link does not resolve | Fix the path, or the file it points at. Only relative links are checked; external URLs are deliberately not, so this never fails on someone else's downtime. |
| `coverage` (cache half) | `.coverage` disagrees with a fresh measurement | `hk run refresh`, then amend. The badge was rendering a stale number. |
| `semver` | the public API broke against the newest `v*` tag | Restore it, or take the version bump semver requires. `crates.io` is immutable, so a wrong rung on V70's ladder cannot be corrected. Two branches report that they examined NOTHING rather than that they found nothing, both on stderr, both passing: `NO BASELINE` when no `v*` tag exists yet, and `BASELINE UNBUILDABLE` when the tag's frozen `rust-version` exceeds the running toolchain -- the second clears itself at the next tag cut on the current toolchain. |
| `coverage` | below the 98% floor | Cover the gap. Lowering the floor needs a reason recorded in `SPEC.md`. |
| hygiene steps | whitespace, line endings, BOM, merge markers, private keys, large files, case conflicts, broken symlinks, smart quotes | The fixable ones fix themselves on commit. A smart quote matters because these docs carry commands meant to be pasted, and the wrong quote fails in the reader's shell rather than here. A private-key hit is never cosmetic -- stop and check what you are about to commit. |

## Releasing

`cargo-release`, configured by `release.toml`. Do not write a release script:
everything one would do, the tool already does, and a second implementation of
one rule set is what `§V64` exists to prevent.

The version bump goes through a **pull request**, because `main` requires one
with twelve passing checks. Only the tail runs from `main` afterwards:

```bash
cargo release hook                 # the gate; dry-run is enough
cargo release tag --execute
cargo release publish --execute
cargo release push --execute
```

Run `hook` first and do not skip it. `tag`, `publish` and `push` do **not**
run `pre-release-hook` — a sequence starting at `tag` publishes whatever the
tree holds. `release.toml` carries the reasoning.

## After fixing

Re-run the gate. Then check whether the failure was worth recording:

- A **bug** gets a row in `SPEC.md`'s `§B`, with the cause -- not the
  symptom.
- If a new invariant would prevent recurrence, add it to `§V` and cite it
  from the `§B` row. This is the point of the spec: the same mistake
  should not be available twice.

[`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md) has the development loop;
[`docs/INTEGRATION.md`](docs/INTEGRATION.md) explains how the gate is wired
and what each set contains; `SPEC.md` `§V` is the law and explains why each
rule exists.
