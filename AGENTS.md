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

One step needs a binary that is not cargo: `cavespec`, which owns this
spec's format and is the only thing checking `SPEC.md`'s structure. The dev
shell provides it from a sibling `../cavespec` checkout, so
`direnv allow` or `nix develop` is enough. It fails hard rather than
skipping when absent -- a skipped structural check reads exactly like a
passing one, and this repo has already paid for that once (`B12`).

## What each failure means

| step | meaning | fix |
|------|---------|-----|
| `fmt` | formatting drift | `cargo fmt` |
| `clippy` | deny-list hit: `unwrap`, `panic`, indexing, silent overflow, or a function/complexity cap | Fix the cause. The caps are a design tool: a function that will not fit usually wants splitting, not a bigger cap. |
| `test` | a test failed | Read the assertion. If it is a race, fix the race -- do not serialise the suite (see `B5`). |
| `doctest` | a doc example failed | Doc examples are compiled. Mark non-code blocks ` ```text `. |
| `itok` | a registered path exceeded its `.context-limits` ceiling | Compact the file. Raising the ceiling is a reviewed decision, not the default move. |
| `cavespec` | `SPEC.md` is not formatted: a hard-wrapped statement, or a line over the cap | `cavespec fmt SPEC.md` joins the wraps. An over-long line is not fixed by the formatter -- split the statement. Raising the cap is a reviewed decision. |
| `cavespec-check` | `SPEC.md` broke a structural rule | Each line names the rule, the line, why it matters, and ranked fixes. A `mechanical` direction is deterministic and safe to apply unattended; a `judgment` one accepts a regression or changes intent, so stop and decide rather than applying it to make the gate green. |
| `ollama` | the cassette-replayed network path broke | The cassette is a *recording*: do not edit `tests/fixtures/` to make a test pass. Re-record deliberately if the protocol really changed. |
| `no-default-features` | the zero-dependency core stopped building | Something outside a `cfg` gate reached for an optional dependency. Feature-gate it. |
| `package` | the published `.crate` would contain the wrong files | Usually an untracked-but-unignored path. Add it to `.gitignore` or `Cargo.toml`'s `exclude`. |
| `rustdoc` | a doc comment breaks docs.rs | Usually `<angle>` text read as an HTML tag. Fence it as ` ```text `. |
| `coverage` | below the 98% floor | Cover the gap. Lowering the floor needs a reason recorded in `SPEC.md`. |
| hygiene steps | whitespace, line endings, BOM, merge markers, private keys, large files, case conflicts, broken symlinks | The fixable ones fix themselves on commit. A private-key hit is never cosmetic -- stop and check what you are about to commit. |

## After fixing

Re-run the gate. Then check whether the failure was worth recording:

- A **bug** gets a row in `SPEC.md`'s `§B`, with the cause -- not the
  symptom.
- If a new invariant would prevent recurrence, add it to `§V` and cite it
  from the `§B` row. This is the point of the spec: the same mistake
  should not be available twice.

`CONTRIBUTING.md` has the development loop; `SPEC.md` `§V` is the law and
explains why each rule exists.
