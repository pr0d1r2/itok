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
| `--all-features` (adds `ollama`) | 44 | the above plus ISC, BSD-3-Clause, CDLA-Permissive-2.0 |

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

### The `ollama` tier — 44 packages

Enabling `ollama` adds 19 packages over the default tier, and brings three
licences the other tiers do not carry. `ureq` is taken with `rustls`, so this
tier includes a TLS implementation and a root certificate bundle:

- **ISC** — `untrusted` and `rustls-webpki`.
- **Apache-2.0 AND ISC** — `ring`, the cryptographic primitives behind
  `rustls`. Note this one contains assembly and C, unlike everything else
  here; if "pure Rust throughout" matters to you, this is where it stops.
- **BSD-3-Clause** — `subtle`, constant-time primitives.
- **CDLA-Permissive-2.0** — `webpki-roots`, which is the Mozilla CA
  certificate set rather than code. Copyright © the Mozilla Foundation.

**This tier used to be larger.** It was 60 packages and carried 19
`Unicode-3.0` licences, none of which came from the backend — `ureq 2`
depended on `url`, `url` on `idna`, and `idna` pulled the whole ICU stack.
Moving to `ureq 3` dropped that chain, so adding a TLS implementation made
the tier **smaller**: 60 to 44, and the `Unicode-3.0` obligation went to
zero. Worth recording because the intuition runs the other way.

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
