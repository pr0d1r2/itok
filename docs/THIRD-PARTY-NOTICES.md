# Third-party notices

`itok`'s dependency footprint is a **choice you make at install time**, not a
fixed property of the crate. So this document is organised by tier rather than
as one flat list — what you owe, and to whom, depends on which one you build.

All figures below are the *runtime* closure (`cargo tree -e normal`), measured
rather than asserted. Dev-dependencies are excluded: they are not distributed
in anything you run.

## The tiers

| build | runtime dependencies | licences involved |
|---|---|---|
| `--no-default-features` | **0** | none — nothing to acknowledge |
| default (`bpe` + `session`) | 25 | 5 permissive expressions |
| `--all-features` (adds `ollama`) | 60 | the above plus `Unicode-3.0` |

### The zero tier is genuinely zero

`--no-default-features` builds the dummy estimator with **no third-party code
at all**. This is not a claim about intent — `no-default-features` is a gate
step, and `nix build .#itok-minimal` builds that configuration in CI, so a
dependency creeping into the core turns the build red rather than being noticed
later.

If you install that tier, this file does not apply to you. Nothing below is
being distributed to you.

### The default tier — 25 packages

Pulled in by `tiktoken-rs` (the `bpe` feature: a real tokenizer, with the
`o200k` ranks embedded so it stays offline) and `serde_json` (the `session`
feature, which parses harness transcripts).

Every package resolves to a permissive licence, in these expressions:

- `MIT OR Apache-2.0`
- `Apache-2.0 OR MIT`
- `Apache-2.0/MIT` *(older syntax, same meaning)*
- `MIT`
- `Unlicense OR MIT`

Where an expression offers a choice, `itok` is distributed under MIT and takes
the MIT option.

### The `ollama` tier — 60 packages

Worth calling out plainly, because the jump is larger than the feature
suggests: **enabling `ollama` more than doubles the dependency count**, adding
34 packages, and introduces a licence family the other tiers do not carry.

The cause is a chain, not the backend itself. `ureq` depends on `url`, `url`
depends on `idna`, and `idna` pulls the ICU stack — `icu_collections`,
`icu_normalizer`, `icu_properties`, `zerovec`, `yoke` and their derives. All
**19 `Unicode-3.0`** packages arrive this way.

- **Unicode-3.0** — the Unicode licence, applying to the ICU crates and their
  embedded data tables. Permissive; requires the notice be retained.
- Copyright © Unicode, Inc. See <https://www.unicode.org/license.txt>.

This is a fact about the cost of a URL parser, not an argument against the
feature. It is recorded here because `SPEC.md` `§V23` makes claims about
`itok`'s dependency shape, and the honest version of that claim is
tier-dependent.

Note that `ureq` is taken with `default-features = false`, so **no TLS stack is
present** — no `ring`, no `openssl`, no certificate chain. `ollama` speaks plain
HTTP to a LAN host by design.

## Reproducing these numbers

Nothing here is hand-maintained, and you should not trust it because it is
written down:

```bash
cargo tree -e normal --no-default-features   # expect: itok alone
cargo tree -e normal                         # the default tier
cargo tree -e normal --all-features          # everything
cargo deny check licenses                    # if you have cargo-deny
```

## Trademarks

Nominative use only; no affiliation or endorsement is implied.

- **Rust** and **Cargo** are trademarks of the Rust Foundation.
- **GitHub** is a trademark of GitHub, Inc.
- **NixOS** and **Nix** are trademarks of the NixOS Foundation.
- **Linux** is a registered trademark of Linus Torvalds.
- **Claude** and **Anthropic** are trademarks of Anthropic PBC.
- **Ollama** is a trademark of Ollama Inc.
- **Unicode** is a registered trademark of Unicode, Inc.

## `itok` itself

Everything in this repository that is not covered above is licensed under the
MIT License — see [`LICENSE`](../LICENSE).
