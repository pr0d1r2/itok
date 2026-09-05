# Built by an LLM, deliberately and in the open

This repository — code, spec, tests and prose — was written by
[Claude Code](https://claude.com/claude-code) running Anthropic's **Claude
Opus 5**. Most commits carry a `Co-Authored-By: Claude Opus 5` trailer; the
current ratio is whatever these two commands say, which is the point of not
writing it down here:

```sh
git log --format=%B | grep -c 'Co-Authored-By: Claude'
git rev-list --count HEAD
```

A human owns every decision, reviews every diff, and is accountable for what
ships.

That is the disclaimer. The rest of this file is why it is stated as a design
note rather than as an apology, and what a reader can check for themselves.

## Why say it at all

Two reasons, and only the first is obvious.

A model writes plausible code, and plausible is not correct. A reader who does
not know how a repository was produced cannot calibrate how hard to look at it.
Saying so is the minimum.

The second is specific to this tool. `itok` exists so that a context cost stops
being a guess: a tokenizer-backed count is measured rather than estimated by a
model or approximated at four characters to the token. A repository arguing
that measurements should replace confident guesses is a repository that has to
be measurable itself — which is why the numbers here live in badges generated
from real runs, and why this page tells you which commands to run rather than
quoting a figure at you.

## The method is spec-driven development

[`SPEC.md`](../SPEC.md) is the law rather than a description written afterwards:
the invariants that must stay true, the tasks that remain, and every bug found
so far paired with the rule that now catches it.

A rule and its checker land in the same commit, because a rule with no runner
gates nothing (`§V17`).

## The guardrails are git hooks that also run on CI

Entering the dev shell (`nix develop`, or `direnv allow`) installs `pre-commit`
and `pre-push`, which run [hk](https://github.com/jdx/hk) against one definition
of the gate in [`hk.pkl`](../hk.pkl) — 26 steps on commit, 32 on push, the slow
half adding coverage, the tarball's contents and the network axis.

[`ci.yml`](../.github/workflows/ci.yml) calls that same definition, so a laptop
and a runner cannot disagree.

The badges in the README are generated from those runs rather than typed there,
because a number typed into prose is true the day it is written and quietly
wrong after. This document follows the same rule and states no counts of its
own.

## The record is deliberately unflattering

`§B` logs what got through. `B16` records that the spec promised TLS the build
could not perform, and that no test covered the path — **found by a reader, not
by the gate.**

That entry is worth more than any green badge on this page. A gate is a claim
about what it catches, and the only honest way to describe one is alongside the
things it did not.

## What a reader should actually check

In the order it matters:

1. **Does the gate run for you?** `nix develop` then `hk check`. If a claim
   here is false, that is where it shows.
2. **Does `§B` look like a real bug log or a curated one?** `B16` is above,
   unedited. Judge the rest by it.
3. **Do the invariants have runners?** An invariant nothing executes is a
   comment with a number on it.
4. **Are the measurements reproducible?** This is a measuring tool. Run it on
   its own repository and see whether the figures it reports about itself match
   the ones it publishes.

## Accountability

The human named in [`LICENSE`](../LICENSE) is responsible for this code,
including the parts a model wrote and the parts nobody caught. "The LLM wrote
it" is an explanation of provenance, never a transfer of responsibility.

Bug reports are welcome and unflattering ones are more useful — see
[`SECURITY.md`](SECURITY.md) for the ones that should not be public, and
[`CONTRIBUTING.md`](CONTRIBUTING.md) for everything else.

## Deeper

[`AGENTS.md`](../AGENTS.md) is the working guide ·
[`CONTRIBUTING.md`](CONTRIBUTING.md) is the loop ·
[`INTEGRATION.md`](INTEGRATION.md) is the gate and its known gaps.
