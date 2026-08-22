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
| default (`bpe` + `session`) | 32 | 6 permissive expressions |
| `--all-features` (adds `ollama`) | 50 | the above plus ISC, BSD-3-Clause, CDLA-Permissive-2.0 |

### The zero tier is genuinely zero

`--no-default-features` builds the dummy estimator with **no third-party code
at all**. This is not a claim about intent — `no-default-features` is a gate
step, and `nix build .#itok-minimal` builds that configuration in CI, so a
dependency creeping into the core turns the build red rather than being noticed
later.

If you install that tier, this file does not apply to you. Nothing below is
being distributed to you.

### The default tier — 32 packages

Pulled in by `tiktoken-rs` (the `bpe` feature: a real tokenizer, with the
`o200k` ranks embedded so it stays offline) and, for the `session` feature,
`serde_json` plus `serde` and `basic-toml`.

**It was 25 in `0.3.0`.** `rate`'s `itok.toml` added `basic-toml`, and reading
its `[rate]` table with `serde`'s `derive` brought `serde_derive` and the macro
machinery it needs — `proc-macro2`, `quote`, `syn`, `unicode-ident` — into the
*runtime* closure, because a proc-macro crate is a normal dependency of the
crate that derives with it. Seven packages, none of which ship code that runs
in the binary, and all of which you are nonetheless entitled to know about.

Every package resolves to a permissive licence, in these expressions:

- `MIT OR Apache-2.0`
- `Apache-2.0 OR MIT`
- `Apache-2.0/MIT` *(older syntax, same meaning)*
- `MIT`
- `Unlicense OR MIT`
- `(MIT OR Apache-2.0) AND Unicode-3.0` — `unicode-ident` only, see below

Where an expression offers a choice, `itok` is distributed under MIT and takes
the MIT option.

**`Unicode-3.0` is back, and it is an AND.** `unicode-ident` carries
`(MIT OR Apache-2.0) AND Unicode-3.0`: the choice applies to the first half
only, so the Unicode licence obligation holds however you take the rest. It
arrived with the `syn` chain described above, which means the `0.3.0` note
further down — that the obligation had gone to zero — is true of `0.3.0` and
no longer true here. One package, in the default tier, and
`https://www.unicode.org/license.txt` is the text it points at.

### The `ollama` tier — 50 packages

Enabling `ollama` adds 18 packages over the default tier, and brings three
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

Both halves of that have since moved: the tier is 50 as of `0.3.1`, and
`Unicode-3.0` returned via `unicode-ident` — one package rather than
nineteen, and from the macro machinery rather than from a URL parser.

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
