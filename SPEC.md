# itok — context-cost estimator

Self-contained spec. `itok` is developed inside a larger workspace but
designed to leave it as a standalone crate (`git subtree split`). It carries
its own law and stands on its own reasoning — no load-bearing reference
outside this directory (V26).

## §G GOAL

Estimate the token / context-window cost of files & changes, from the
command line, with `git`- and `du`-shaped grammar so a human or an agent
who knows those tools needs no teaching.

## §C CONSTRAINTS

- Rust. One bin: `itok`. MIT licensed.
- Offline & deterministic by default; runs fully in a NETWORKLESS SANDBOX
  (V34). Network only behind the opt-in `--ollama` tier (LAN, keyless),
  ⊥ in core, ⊥ any paid/authed cloud API (V35).
- ASCII-only source (Trojan-Source, LLM-friendliness).
- Zero deps on any host-project internals ⇒ extraction is a move.
- Estimation, ⊥ measurement: no public Claude tokenizer exists ∴ the
  default is a labelled proxy, never a claim of truth.

## §I INTERFACE

- `itok estimate <path>` — `-s` summarize · `-h` human · `--top N` ·
  `--budget N` · `--bpe` · `--format json` · `-C <dir>`.
- `itok diff [<ref>|<A> <B>|<A>..<B>] [-- <path>]` — `--staged` ·
  `--budget N` · `--exit-code` · `--bpe`.
- `itok show [<commit>] [-- <path>]` | `itok show <commit>:<path>` — one
  commit's per-file token delta (default HEAD); `<commit>:<path>` = a
  blob's cost at a ref. Report-only · `--bpe` · `--format json`.
- `itok check` — reads `.context-limits`, pinned `--bpe`, pass/fail.
- `itok doctor <path>` — `--model X` · `--window N` · `--format json` ·
  `-C <dir>`. Advisory: fit-to-window · balance · noise · estimate
  confidence. Reports & suggests; never gates.
- `itok log <path>` — `-n N` · `<A>..<B>` · `--reverse` · `--since` ·
  `--bpe` · `--format json`. Per-commit cost + delta; report-only.
- `itok fit --window N [paths]` — `--by size` · `--bpe` · `--format json`.
  Greedy subset that fits the budget; emits a pipeable path list.
- `--ollama [HOSTS]` (estimate/doctor) — exact counts via the
  server's tokenizer + live model→window. Each host `[scheme://]host[:port]`
  (default `11434`); a comma-list, `-` (stdin), or `.context-hosts`;
  honors `OLLAMA_HOST`. Fleet = union of models. Network, the SLOWEST-REMOTE
  rung (V4); never on `check`/`log`. No CIDR (V24), no `--ollama-port` (V25).
- Config: `.context-limits` (per-path ceiling), `.context-models`
  (model → encoding table).
- Exit: 0 ok · 1 breach/delta · 2 usage · 7 network (`--ollama`).

## §V INVARIANTS

V1: **convention over novelty.** Default to the form `git`/`du`/`rg`
already use, even when a novel one is "better". Convention lives in the
model's pretrained prior & in muscle memory ∴ costs 0 to invoke. Novelty
is taxed on 4 recurring axes — teaching (paid every session), retry
(wrong-guess round-trips), surprisal (low-probability form ⇒ the model
hedges), parsing (novel output re-inferred). Ship novelty only when
value-added > cost-to-teach.
V2: **near-collision is the expensive failure** ∴ match a convention's
SEMANTICS exactly | be visibly different — never almost. A `diff` that
looks like `git diff` but acts differently invokes the prior then
violates it ⇒ costs more than honest novelty.
V3: **name = estimation; the number self-describes.** Output ALWAYS
names its unit & method: `~166k itok (bytes/4)`. `itok` = INPUT tokens
(what a file costs fed INTO a model, ⊥ output/generation) ∴ an agent
needs no external knowledge — the number says what it is (V15); `i`/`o`
is the most burned-in split in computing (V1). The `~` marks a CRUDE
estimate — dummy tier ONLY; `--bpe`/`--ollama` drop it (`166k itok
(o200k)`, `166k itok (exact)`) ∵ they are a real tokenizer's true count.
Tilde presence = the estimate-vs-truth signal at a glance. (`--exact` was
the old flag name for the true rung; it is now `--ollama`, V35.) json (T3)
carries the same intent structurally (`unit`, `estimated`), ⊥ w/ a tilde
in a numeric field (V9).
V4: **precision ladder, ordered FASTEST-LOCAL → SLOWEST-REMOTE.** `dummy`
(bytes/4 + word proxy, zero-dep, instant) | `--bpe` (tiktoken, offline,
deterministic, the honest proxy) | `--ollama` (a local model's OWN
tokenizer, LAN localhost, keyless, exact for that model — V22). Each rung
labelled; `dummy` & `--bpe` are honestly "estimation", `--ollama`
transcends it. The local rungs (`dummy`, `--bpe`) are the CORE & the only
tiers a networkless sandbox needs (V34); `--ollama` is the opt-in remote
rung. NO cloud/paid-API rung — a provider `count_tokens` behind an API key
is rejected (V35).
V5: **only `check` gates.** It pins `--bpe` ∴ deterministic & cacheable
by content hash. `estimate` & `diff` are report-only, exit 0. A gate that
varies per run is ⊥ a gate.
V6: **prefix-inference, read-only verbs only.** Unambiguous prefix ⇒
resolve silently (`itok e`, `itok di`, `itok ch`). Ambiguous ⇒ error listing
candidates, never silent-pick. Non-prefix typo ⇒ SUGGEST (edit-distance),
⊥ run. Canonical names are the full words; prefixes are convenience,
never promised. Any future MUTATING verb requires its full name — no
inference into a write.
V7: **`diff` mirrors `git diff` arg-forms verbatim** — working-tree,
`--staged`, `<A> <B>`, `<ref>`, `-- <path>`, `--exit-code`. Zero new
mental model (V1).
V8: **`estimate` mirrors `du`** — `-s`/`-h`/`--top` — & operates on
git-tracked files by default (like `rg` respects `.gitignore`); explicit
paths reach untracked files directly. `du`'s `-d` (recursion depth) & `-a`/
`--all` are ⊥ shipped: `estimate` costs a FLAT fileset (git-tracked list |
argv), ⊥ a recursive walk, so depth has nothing to recurse & untracked is
a named path away. Add either only when a real need appears.
V9: **porcelain/plumbing split.** Human table MAY evolve; `--format json`
(one object per file) is a STABLE contract. Agents parse json; the pretty
table stays cosmetic.
V10: **`.context-limits` is opt-in**, ⊥ fail-by-default. Unlike a
repo-guard registry, `itok` runs on arbitrary repos ∴ an unregistered path
is simply unchecked. `check` gates only what the user registered.
V11: **`.context-models`: unknown model ⇒ FAIL, name an encoding.** No
silent fallback to a default BPE — a silent wrong-tokenizer is the V2
failure. A new model is a reviewed row.
V12: **BPE vocab VENDORED**, with license provenance, ⊥ fetched at
runtime. Offline-first + opensource both demand the data ship in-tree.
V13: **zero host-internal deps** ∴ extraction = `git subtree split -P
crates/itok`, ⊥ surgery. Own `Cargo.toml`, own license, own `SPEC.md`,
own vocab. Nothing imported from the monorepo.
V14: **self-checked & dogfooded.** `cavekit-spec` validates THIS
`SPEC.md`; while `itok` lives in the host it runs as a host guard unit
(`itok check` in pre-commit, invoked by resolved path). It wears both
hats — product & guardrail — like the other extractable crates.
V15: **`itok` dogfoods its own metric** — it measures token cost ∴ it must
be the cheapest tool to learn (V1). A token tool that needed teaching
would refute its own thesis.
V16: **budget IS the switch** — `--budget N` on `estimate`/`diff` ⇒ exit
nonzero when a file (or a change's delta) exceeds N. A threshold intends
a gate ∴ no separate `--guard` boolean. Name = `budget`: ⊥ `--guard`
(novel jargon, V1), ⊥ `--max-tokens` (near-collides w/ the API
`max_tokens` = GENERATION cap, opposite meaning ∴ V2's expensive
failure). Complements `check`: `--budget` is the inline one-shot (no
registry, the CI one-liner), `check` is committed policy
(`.context-limits`). Both gate; both pin `--bpe` when accuracy matters.
`--budget` on `diff` needs no registry ∴ "no commit adds > N tokens" is a
review gate anyone can drop into CI.
V17: **`doctor` = advisory pre-flight** — "is this context healthy to
hand a model?" Answers *should I?* where `estimate` answers *how much?* &
`check` answers *passes policy?*. Report-only, suggests fixes; ⊥ a gate.
Name = `doctor` ∵ convention (`brew`/`flutter`/`npm doctor`) is in the
prior (V1); ⊥ `sane` (novel, collides w/ the SANE scanner API, reads
wrong as a verb). Composes itok-NATIVE signals ONLY: fit-to-window,
budget balance (one file dominating), noise ratio (generated/binary/lock
share), estimate confidence (dummy-vs-bpe spread — the one signal only
itok has, ∵ it owns the estimators). BOUNDARY: dup-detection & vocab/TTR
are separate tools — `doctor` stays a thin composer, ⊥ grows
tentacles.
V18: **window override** — `--window N` gives context capacity raw;
`--model X` resolves it from `.context-models` (extended to model →
encoding + window). Explicit `--window` wins over `--model`. Unit =
DECIMAL tokens (`1M` = 1_000_000, `200k` = 200_000), one parser shared
w/ `--budget` (one unit grammar, less to hold). Name `--window` ⊥
`--size` ∵ "size" primes BYTES (its meaning in `du`/`ls`) but a window is
TOKENS ⇒ V2 near-collision. `--size` MAY be a silent alias for muscle
memory, ⊥ the documented name.
V19: **`log` = cost across history** — the 5th question (*how did it
evolve?*), the creep curve `diff`'s 2 points ! show. Mirrors
`git log <path>` verbatim (V1): 1 line/commit that touched the path —
sha·date·subject + absolute tokens + delta (like `--stat`'s +/-);
`-n N` · `<A>..<B>` · `--reverse` · `--since`. Report-only, exit 0 — raw
data like `git log`; judgment (flag N rises) stays OUT (scope).
Defaults to `dummy` ∵ whole history = N blobs & `git cat-file -s` gives
blob bytes directly (near-free); `--bpe` opt-in for accurate absolutes at
N tokenizations. Per-blob-hash cache ∴ a blob is never re-estimated.
V20: **`fit` = greedy pack, ⊥ knapsack** — select the subset of
candidates that fills a `--window` budget. Knapsack (NP-hard) only bites
when items approach capacity; context files are KB against a 1M-token
window ∴ each is a tiny fraction & greedy-by-order ≈ optimal (fractional
≈ integral when item ≪ capacity). Order candidates (argv/manifest order
default; `--by size` to fit-most), take while running total ≤ window,
emit survivors. Reuses `--window` parser (V18), estimate tiers (V4,
dummy default), tracked-default (V8). Output = a pipeable PATH LIST
(NUL/newline, `git ls-files`/`rg -l` shape) ∴ `itok fit --window 200k
src/ | xargs cat` assembles a context bundle under budget — useful w/o
any programmatic assembler. Report-only, exit 0. Name = `fit` (the
question *what fits?*) ⊥ `pack` (implies the bin-packing optimization we
⊥ do). It SELECTS ⊥ reports ∴ its own category beside the 5 report verbs.
V21: **optimal fit DEFERRED (⊥ rejected)** — true 0/1 knapsack (max
priority under budget for near-capacity large items) & per-file priority
tables. Greedy (V20) covers the KB-files-vs-1M-window case; the solver
earns its complexity only when large near-capacity items make greedy
visibly wrong. Trigger: a real fileset where greedy drops a
better-fitting combination.
V22: **`--ollama [HOST]` = self-hosted exact + live windows** — THE
slowest-remote rung of the ladder (V4), keyless & free: `localhost:11434`
plain HTTP (no TLS, no API key, no payment). The ladder is dummy | bpe |
`--ollama`; there is no cloud/paid rung (V35). Gives 2 things: (1) EXACT
counts via the
model's own tokenizer (`prompt_eval_count` from a `num_predict:0`
generate) ∴ a local llama/qwen is measured true, ⊥ by the o200k proxy
that is wrong for it; (2) live model→window from `/api/tags` + `/api/show`
(`context_length`). `doctor --ollama` w/o `--model` enumerates ALL local
models & reports fit-% against each — the answer no static table gives.
Honors `OLLAMA_HOST` (V1: Ollama's own convention); `--ollama` bare =
`OLLAMA_HOST`|`localhost:11434`, `--ollama HOST` overrides. Applies to
`estimate` & `doctor` -- the verbs where an EXACT count earns its network
cost ("how much exactly?" / "does it fit this model's LIVE window?").
`diff` (a gate) & `fit` (a packer) stay on the margin'd `--bpe` proxy: a
gate wants a CONSERVATIVE estimate, ⊥ a true count (V36), so exact adds no
value there -- exact on diff/fit DEFERRED, ⊥ rejected (trigger: a real
need for exact deltas / exact packing). NEVER `check` (V5 determinism) |
`log` (history×network). Precedence: live endpoint > `.context-models` > error;
the table stays the offline fallback & the only source for cloud windows.
Non-deterministic + network ∴ all remote-tier rules (V4/V23). Content leaves
the process to the endpoint — LAN self-hosted is the PRIVATE path vs
cloud, a conscious tradeoff.
V23: **network tier feature-gated** — `--ollama` lives behind a cargo
feature (`ollama`) w/ a tiny blocking client (e.g. `ureq`, ⊥ an async
runtime; `localhost` HTTP ∴ no TLS stack). ∴ the core ships ZERO network
deps, stays small for extraction (V13), & runs in a networkless sandbox
(V34). Offline-first is the default BUILD, ⊥ merely the default flag. NO
cloud client ships at all (V35).
V24: **`--ollama` takes explicit HOSTS, ⊥ a CIDR — no in-tool scanning.**
A subnet probe is a PORT SCAN: hostile/noisy/IDS-flagging on any network
you ⊥ fully own, sometimes a policy/legal problem — a headline liability
for an opensource token tool that buys nothing. Scope-alien too (service
discovery ≠ token measurement) & slow+non-deterministic (breaks
snappiness). V1: the convention for "find services" is advertise (mDNS) |
explicit list, ⊥ scan. So `--ollama` accepts a host, a comma-list, `-`
(stdin), or `.context-hosts`; a fleet is the UNION of models & windows
across named hosts (`doctor --ollama box1,box2` = which model anywhere
holds this). Discovery, if truly wanted, is COMPOSED — `nmap -p11434 …
| … | itok doctor --ollama - .` — a dedicated scanner pipes hosts in; itok
never ships a scanner. Considered & rejected, escape hatch recorded.
V25: **port lives in the host (`host[:port]`), ⊥ a `--ollama-port`
flag.** `OLLAMA_HOST=1.2.3.4:6666` is Ollama's own form ∴ `host:port` is
already in the prior (V1) & universal (ssh/redis/pg/git-remote). A
separate `--ollama-port` can't express per-host ports in a fleet list
(`box1:11434,box2:6666`) & works in none of stdin/`.context-hosts`.
Default port `11434`; a `scheme://` prefix carries TLS for a proxied
server — same slot, no new flag. Considered & rejected.
V26: **provenance is a DEV-TIME note, kept OUT of the shipped spec.** A
reference to the origin repo's invariants is see-also / derived-from,
NEVER load-bearing — each itok invariant stands on its own reasoning. ∴
the SHIPPED, public `SPEC.md` names NO origin-repo invariant number: a
cross-repo pointer targets a PRIVATE repo the reader can't see & leaks its
numbering, a bare `Vxxx` collides w/ itok's own namespace & reads broken.
The reasoning a pointer once cited is folded INLINE & self-contained. The
scrub lives IN the monorepo (this file is kept public-clean in place) ∴
extraction stays a pure MOVE (V13), ⊥ surgery. Lineage lives in git
history & commit trail, ⊥ the shipped text.
V27: **itok self-guards — its guard config TRAVELS.** The fractal: every
extractable unit is a mini-repo. crates/itok/ mirrors a repo root — own
`SPEC.md`, `.file-limits`, `lexicon.txt`, `coverage-baseline.json`,
`Cargo.toml`, `rustfmt.toml`, `clippy.toml`, + a `.uow/`. ∴ `subtree
split` carries the GUARDS w/ the code; itok guards itself standalone, ⊥
orphaned. Config that CHANGES a verdict (`rustfmt.toml`'s `max_width`,
`clippy.toml`'s thresholds) ! travel beside `Cargo.toml`'s `[lints]`, else
the standard gate (V31) silently weakens on extraction — the extracted
`cargo fmt --check` reformats to defaults & fails (B1). Reconciles V13
(self-contained) & V14 (dogfooded): in-repo the host RUNS the guards, but
the config they read is itok's own.
V28: **`.uow/` = the unit DECLARATION, root = the config DATA.** `.uow/`
holds `unit.yml` (itok's ops × itok-scoped globs × phases × deps) +
`flake.nix`/`flake.lock` (detachable toolchain — build on any nix box) +
`.envrc`. The baselines (`.file-limits`, `lexicon.txt`,
`coverage-baseline.json`, `SPEC.md`, `Cargo.toml`) sit at crates/itok/
ROOT — where tools expect them & where they land as root after
extraction. `.uow/` = how-to-guard; root = what-to-guard-against.
V29: **`unit.yml` phase-splits itok's STANDARD guards** — fmt · clippy ·
nextest(lib) on pre-commit; nextest(e2e) · `llvm-cov --fail-under-lines`
on pre-push; heavier axes on ci. Cargo-native ONLY (V31) ∴ the split runs
identically in-repo & extracted with no host bin present; `cargo run`
never (a resolved-path bin, ⊥ a nested cargo). The custom guards (hygiene, lexicon, cavekit-spec,
ascii) are ⊥ here — they run in the monorepo via root config (V31).
V30: **in-repo, itok's config is honored by NEAREST-config cascade** (a
host feature): a file under crates/itok/ is measured against crates/itok/
baselines, ⊥ root's (the editorconfig/gitignore cascade, V1). ∴ itok
growth is reviewed vs itok ceilings & root config retires its
`crates/itok/**` entries. Bootstrap (single root config) → cascade is the
dir-walking native tier the host adds; until then root governs by
exact-path (the current `crates/itok/SPEC.md` entry).
V31: **the traveling gate is STANDARD-ONLY; custom guards are
monorepo-only.** The fancy guards are HOST crates (`repo-guard`,
`cavekit-spec`) — they do ⊥ travel with `crates/itok`, so `unit.yml` &
CI use cargo-native tools alone (fmt, clippy, nextest, llvm-cov). Coverage
is a `--fail-under-lines` FLOOR, ⊥ the per-file `coverage-freeze` ratchet
(a host bin); `coverage-baseline.json` travels as reference, ⊥ enforced
in the extracted repo. Two-tier by design: the monorepo is itok's
full-guard home (root config, all guards, where it is developed); the
public repo runs the standard gate. A graft lands public, flows back, &
hits the full guards in the monorepo — external contribution, internal
tending (the "tend in the loop by graft" loop). Bringing the toolkit
(extracting `repo-guard`/`cavekit-spec`) is deferred; trigger = the public
repo wanting the fancy guards independently.
V32: **`diff`/`show`/`log` = git's triad, matched to git's CLI.** Three
questions at three granularities, git's own split (V1): `diff` = any two
points (working · `--staged` · `<A> <B>` · `<A>..<B>`); `show` = ONE
commit, deep + per-file (default HEAD, like `git show`); `log` = many
commits, one delta-line each. `show <A>..<B>` is ⊥ a thing — a range is
`diff` (endpoints) | `log` (walk), & routing it through `show` would be a
V2 near-collision. `show <commit>:<path>` = a blob's cost at a ref
(`git show`'s object syntax). ∴ each verb means exactly what its git
namesake means, no almost.
V33: **`--` path filter & `A..B` are GIT-UNIVERSAL, ⊥ per-verb reinvented.**
`-- <path>` narrows show/diff/log to a pathspec (git's rev-vs-path
separator); itok is lenient when a bare arg is plainly ⊥ a rev
(`itok show <sha> SPEC.md`). `A..B` selects endpoints (`diff`, ≡ `A B`) |
a range (`log`), per git. All 3 verbs stand on ONE primitive — `gitref`:
the token cost of a file AT a commit (`git cat-file` blob → dummy|bpe).
diff compares 2 refs, show does one commit's per-file deltas, log walks a
sequence; build it once, share it (⊥ 3 copies).
V34: **runs fully in a NETWORKLESS SANDBOX.** Core/default build ! need
ZERO network & ZERO extra auth. Tiers stack fastest-local → slowest-remote
(V4): `dummy`·`--bpe` = local core (sandbox-safe, always built); `--ollama`
= opt-in remote rung, feature-gated (V23) & doc'd as a SLOWER tier of
operation. A no-net sandbox runs the local tiers & every gate (`check`,
`--budget`) unchanged ∵ gates pin `--bpe` (V5) & never touch net. This is
the north-star axis: content-locality > precision (V36).
V35: **no paid API, no extra auth — cloud `count_tokens` REJECTED.** A
provider exact-count (e.g. anthropic `/v1/messages/count_tokens`) needs an
API KEY (extra auth) + a paid-tier credential + EGRESSES file content to a
third party (DLP/compliance) ∴ ⊥ core, ⊥ any default. The exact rung is
`--ollama` instead (local model's own tokenizer, keyless, free, on-LAN —
V22); `o200k` (`--bpe`) is the honest LOCAL proxy for cloud models
meanwhile. Considered & rejected, escape hatch recorded (like V24/V25): a
cloud rung, if ever wanted, is a separate opt-in feature that ! never be
default & ! shout its content egress.
V36: **guardrail wants a CONSERVATIVE estimate, ⊥ a true count.** The
gate's job = catch context OVERLOAD, ⊥ report exact tokens ∴ a margin'd
local estimate (`--bpe` + headroom under the window) is CORRECT, & chasing
cloud-exactness was the wrong axis (V35). Exactness ≠ goal; non-overload =
goal. `--ollama` exact is a CONVENIENCE for model-fit reporting
(doctor/fit), ⊥ a requirement of the gate. Model-fit selection reads a
STATIC `.context-models` window (T14, e.g. `qwen3-coder:30b` = 256k) — no
net needed to know a model's ceiling.
V37: **the GUARD travels ∴ tests are LAYOUT-AGNOSTIC.** A test ⊥ assume
the monorepo path — `CARGO_MANIFEST_DIR.parent().parent()` as the repo
root, `crates/itok/…` pathspecs — ∵ extracted, the crate IS the repo root
& those overshoot the tree / name absent paths (B2). Derive the git root
& the crate's prefix at RUNTIME (`git rev-parse --show-toplevel` /
`--show-prefix`) so ONE test passes at `<repo>/crates/itok` AND standalone.
V13's "extraction = a move" binds the TESTS too, ⊥ only src; the rehearsal
(`subtree split` → clone → run `.uow` guards) is how it is proven (T11).
LAYOUT is not the only ambient a test ⊥ assume: repo STATE too — the live
`HEAD~1..HEAD` is ⊥ presumed SUBSTANTIAL (a near-zero-delta config commit
as HEAD broke a `--budget` breach assertion, B3); drive a delta test from a
CONTROLLED range (empty tree → HEAD adds the whole repo), ⊥ ambient history.
V38: **a network backend is CASSETTE-REPLAYED in the gate, ⊥ live.**
Record the real HTTP interactions ONCE into a standard `vcr-cassette` file
(portable, VCR-interop), replay them offline THROUGH the real client
(ureq) via a local stub (a `TcpListener` serving the recorded response) so
the full path — client → parse → render — is deterministic & CI-safe with
NO server. The LIVE endpoint check is a `#[ignore]` test, run by hand
(`--ignored`), ⊥ in the gate. The cassette dep is DEV-only (serde/chrono/
url, ⊥ tokio/async) ∴ V23/V13 hold: the shipped binary stays network-free
& tiny. rvcr rejected — it is reqwest+tokio = an async runtime, V23's
forbidden shape. ∴ every network feature ALSO runs a CI axis (`--features
ollama` clippy+nextest in `.uow`) so the replayed path ⊥ rot.
V39: **publishable = public gate green + honest SemVer + zero private
reference.** crates.io needs 4: (1) the STANDARD gate reproduces on plain
rustup CI (V31) — fmt · clippy · test · `--features ollama` · `llvm-cov
--fail-under-lines`, NO nix & NO host bin, git history fetched FULL
(diff/show/log read `HEAD~n`); (2) SemVer tells maturity TRUE — `0.8.0` =
stable but pre-1.0 (API MAY move; 1.0 = finished), ⊥ default `0.0.0`; (3)
metadata complete (`repository`·`homepage`·`documentation`·`readme`·
`keywords`·`categories`·`rust-version`) so crates.io/docs.rs render; (4)
the shipped `.crate` names NO origin repo (V26) & `exclude`s non-consumer
files (`.uow/`, baselines). Publish = `subtree split` (V13) of a crate
carrying all four.
V40: **docs render from ONE command registry — `itok docs` emits it, a
guard FREEZES it.** itok hand-rolls args (no clap) ∴ verb/flag text
scatters across help strings & rots alone (the README did). One registry
(verb → synopsis · flags · exit) feeds EVERY view — `--help`, `itok docs`
(markdown reference), later man — so a new verb is a ONE-place edit.
READ-ONLY, to stdout (V6): `itok docs > README.md` is the USER's redirect,
⊥ the tool writing. Generated = REFERENCE; the NARRATIVE (intro, ladder,
mermaid) stays hand-written ABOVE it. A guard diffs `itok docs` vs the
committed reference ∴ staleness FAILS the gate (coverage-freeze, V14) —
docs cannot rot, ⊥ merely regenerable.

## §T TASKS

| id | scope | tasks | done-when |
|----|-------|-------|-----------|
| M1 | offline core — estimate + report | T1, T2, T3, T4, T12 | `itok estimate`/`itok e` green on a real tree, json stable, `--budget` gates |
| M2 | full ladder + gate + extract | T5, T7, T8, T9, T10, T11, T13, T14, T15, T16, T17, T18, T19, T20, T21, T22, T23, T24 | `--bpe`/`--ollama`/`diff`/`check`/`doctor`/`log`/`fit` done, subtree-split rehearsed |
| M3 | publish — crates.io-ready, public CI | T25, T26, T27, T28, T29 | `cargo publish --dry-run` clean, public CI green, no origin-repo ref shipped, docs regenerate, `itok` installs |

T1|x|crate skeleton: standalone bin, Cargo.toml, MIT license, min deps|V13
T2|x|`estimate` dummy tier: bytes/4 + word proxy, du flags, tracked-by-default|V4,V8
T3|x|`--format json` stable output contract|V9
T4|x|prefix-inference + typo-suggest, read-only verbs only|V6
T5|x|`--bpe` tier: tiktoken, vendored vocab, deterministic|V4,V12
T7|x|`.context-models` table; unknown model ⇒ fail|V11
T8|x|`diff` verb: git arg-forms (`A B`/`A..B`/`--staged`/`-- path`) + `--exit-code`|V7,V2,V32,V33
T9|x|`check` verb: `.context-limits`, pinned bpe, exit codes|V5,V10
T10|x|dogfood: `itok` as host guard unit, pre-commit `itok check`|V14
T11|x|extraction dry-run: build & GUARD isolated, zero host deps, `subtree split` rehearsal|V13,V27,V37
T12|x|`--budget N` inline gate on estimate & diff: exit-on-breach, report overage|V16,V5
T13|x|`doctor` verb: advisory, native signals (fit/balance/noise/confidence)|V17,V18
T14|x|`.context-models` carries per-model window; `--window` override, shared unit parser|V18,V11
T15|x|`log` verb: per-commit token cost + delta across history, git grammar (`A..B`, `-- path`)|V19,V32,V33
T16|x|`fit` verb: greedy subset under `--window`, pipeable path-list output|V20
T17|x|`--ollama` backend: exact via `prompt_eval_count`, live model/window discovery, feature-gated|V22,V23
T18|x|`--ollama` `host[:port]`-list + `.context-hosts` + stdin; fleet union; ⊥ CIDR/port-flag|V24,V25
T19|x|`crates/itok/.uow/unit.yml`: itok guard units, ops×globs×phases, resolved-path bins|V28,V29
T20|x|`crates/itok/.uow/flake.nix`+`flake.lock`+`.envrc`: detachable toolchain|V28
T21|x|itok baselines at crates/itok/ root: `.file-limits`·`lexicon.txt`·`coverage-baseline.json`|V27
T22|x|`gitref` primitive: token cost of a file AT a commit (`git cat-file` → dummy/bpe), shared by diff/show/log|V33
T23|x|`show` verb: one commit's per-file delta (default HEAD), `-- path`, `<commit>:<path>` blob|V32,V33
T24|x|cassette-replay the ollama backend: `vcr-cassette` dev-dep + ureq replay stub + fixtures (`/api/tags`·`/api/show`·`/api/generate`); live smoke → `#[ignore]`; `ollama` CI axis in `.uow`+hook; fold into `src/ollama/`|V38,V22,V23
T25|x|metadata: `Cargo.toml` version `0.8.0` + `repository`·`homepage`·`documentation`·`readme`·`keywords`·`categories`·`rust-version`, `exclude` non-consumer files|V39,V13
T26|x|public-clean provenance: scrub SPEC + src comments of origin-repo names & cross-repo invariant pointers (fold inline), header un-names the workspace, `exclude=[".uow"]`|V26,V39
T27|x|bare-rust `.github/workflows/ci.yml`: fmt·clippy·test·`--features ollama`·`llvm-cov --fail-under-lines 99`, `fetch-depth:0`, no nix/uow|V39,V31,V38
T28|x|assemble README = hand narrative + the `itok docs` reference block + badges (post-ORG); `CHANGELOG.md`; crate rustdoc for docs.rs|V39,V40,V15
T29|x|`itok docs` verb: ONE command registry (verb·synopsis·flags·exit) renders `--help` + a markdown reference; read-only to stdout; guard diffs it vs README ∴ docs can't rot|V40,V6,V9

## §B BUGS

id|date|cause|fix
B1|2026-07-24|extraction: `rustfmt.toml`/`clippy.toml` root-only ⇒ standalone `cargo fmt --check` reformats (default width 100 vs 80); traveling gate ⊥ reproduces verdict|V27
B2|2026-07-24|extraction: git-command tests use `CARGO_MANIFEST_DIR.parent().parent()` as repo root + hardcode `crates/itok/…` paths ⇒ standalone `cargo nextest` fails|V37
B3|2026-07-24|`diff --budget` test asserted a breach on live `HEAD~1..HEAD`, presuming a substantial last commit; a near-zero-delta ceiling bump as HEAD gave no breach ⇒ exit 0 ≠ 1, blocking every commit until HEAD grew|V37
