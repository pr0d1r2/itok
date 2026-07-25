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
- Runtime axis (V41) reads the HARNESS's own on-disk transcript,
  READ-ONLY. ⊥ store content, ⊥ egress, ⊥ touch credentials (V43/V45).
- Enforcement is opt-in & ADAPTER-shaped: no daemon, no server, no
  in-flight interception, ⊥ in the request path (V52/V53/V58).

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
- `itok trace [<session>]` — `-n N` · `--since D` · `--reverse` ·
  `--format json`. Runtime LOAD EVENTS, 1 line each, chronological.
  Report-only.
- `itok top [<session>] [-- <path>]` — `-h` · `-s` · `--top N` ·
  `--cost` · `--format json`. Ranked context OCCUPANCY + dup · stale ·
  cache columns. `-- <path>` = that path's loads (per-path attribution;
  ⊥ a separate `blame` verb, V46). Report-only.
- `itok calibrate [<session>]` — estimate-vs-actual factor from CLEAN
  samples only, reports `n`; `--format json`. Report-only (V48).
- `itok cap [N]` — stdin→stdout token filter. `--strip` · `--dedup` ·
  `--elide` · `--outline` (the reduction ladder, V50) · `--footer
  human|json`. Announces its elision; resumable (V49/V51).
- `itok guard` — hook ADAPTER: harness hook JSON on stdin → decision
  JSON on stdout; reads `.context-policy`. The runtime gate (V52/V53).
  Signals in JSON, ⊥ via exit code — the harness reads stdout.
- Config: `.context-limits` (per-path ceiling), `.context-models`
  (model → encoding + window + optional RATE column, `--cost`),
  `.context-policy` (per-glob/per-tool budget · pins · fuse tiers).
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
`SPEC.md`, `lexicon.txt`, `coverage-baseline.json`, `Cargo.toml`,
`rustfmt.toml`, `clippy.toml`, `.gitignore`, + the
toolchain & gate it runs on (root `flake.nix`/`flake.lock`/`.envrc` +
`.github/workflows/ci.yml`, V62). ∴ `subtree split` carries the GUARDS w/
the code; itok guards itself standalone, ⊥ orphaned. Config that CHANGES a verdict (`rustfmt.toml`'s `max_width`,
`clippy.toml`'s thresholds) ! travel beside `Cargo.toml`'s `[lints]`, else
the standard gate (V31) silently weakens on extraction — the extracted
`cargo fmt --check` reformats to defaults & fails (B1). Reconciles V13
(self-contained) & V14 (dogfooded): in-repo the host RUNS the guards, but
the config they read is itok's own.
V28: **the unit DECLARATION is MONOREPO-ONLY; the standalone repo is an
ORDINARY repo.** A host-specific declaration (`.uow/unit.yml` — ops ×
globs × phases × deps) describes work for a HOST RUNNER, & no such runner
exists in the extracted repo ∴ shipping it there is a DEAD FILE that
reads as law: a contributor sees a gate declaration, edits it, & nothing
runs it (worse, it can DRIFT from the live gate & carry a stale threshold
— exactly B4's shape, a `99` floor surviving beside CI's corrected 98).
∴ the public repo carries only what plain tools read: root `flake.nix`/
`flake.lock`/`.envrc` (V62) & `.github/workflows/ci.yml` (V31). The
baselines (`lexicon.txt`, `coverage-baseline.json`, `SPEC.md`,
`Cargo.toml`) sit at the crate ROOT — where tools expect them
& where they land as root after extraction. Root = what-to-guard-against;
the how-to-guard is the WORKFLOW here & the unit declaration THERE. Same
ops either way (V31) — one verdict, two runners.
V29: **the phase split is the HOST RUNNER's, ⊥ the repo's** — fmt ·
clippy · nextest(lib) on pre-commit; nextest(e2e) · `llvm-cov
--fail-under-lines` on pre-push; heavier axes on ci. That ordering is an
optimization for an interactive hook (fail fast, cheapest first) ∴ it
lives with the runner that HAS hooks (the monorepo). CI has no such
gradient — it runs everything on every push — so the standalone gate is
FLAT & complete, ⊥ phased. Cargo-native ONLY (V31) either way ∴ the same
ops give the same verdict under both; `cargo run` never (a resolved-path
bin, ⊥ a nested cargo). The custom guards (hygiene, lexicon,
cavekit-spec, ascii) are ⊥ in either — they run in the monorepo via root
config (V31). A standalone contributor who wants the gradient uses a git
hook | a task runner — a repo-level convention, ⊥ a specced artifact.
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
in the extracted repo. The floor is the UNIT's OWN standalone coverage
(itok ≈ 98%), ⊥ the monorepo WORKSPACE total (sibling crates inflate that
to 99%, B4): a floor copied from the aggregate fails the single crate.
Two-tier by design: the monorepo is itok's
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
(diff/show/log read `HEAD~n`); (2) SemVer tells maturity TRUE — the version
is a RUNG on the ladder (V70), ⊥ default `0.0.0`; publication itself is
`0.7.0`, ⊥ whatever number the crate happens to carry; (3)
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
V41: **second axis — RUNTIME, beside the static one.** V1-V40 answer
about FILES, ex ante ("what will this cost?"). The runtime axis answers
about a CONTEXT, ex post ("what actually got loaded, by what, at what
cost?"). Same estimator engine (V4), new frontend. It earns its own
verbs ∵ input tokens are billed EVERY turn while output tokens spend
ONCE ∴ a file loaded at turn 3 is still charged at turn 40 & runtime
waste COMPOUNDS. `itok` = input tokens (V3) ∴ this axis is the name's
other half, ⊥ scope creep.
V42: **observe before enforce** — the ORDERING law & the modularization
law. Telemetry (read-only, post-hoc, zero interception) ships FIRST &
usable ALONE; reduction ships next & usable ALONE (a pipe filter);
enforcement ships LAST, tuned from measured numbers. ∵ a fuse threshold
guessed w/o data has false-positives that cost MORE than the waste it
prevents (a denied legit read = retries + rephrase = burned turns). Each
rung ! stand on its own & be shippable alone; none may require the next.
V43: **the transcript is the GROUND TRUTH & is READ-ONLY.** Session data
comes from the harness's own on-disk transcript (Claude Code: JSONL under
`~/.claude/projects/…`), ⊥ from interception. It already carries the real
API `usage` ∴ ACTUAL, ⊥ estimate — the closed loop for free. itok NEVER
writes/moves/mutates it. FOREIGN & unversioned schema ∴ parse
defensively: unknown fields ignored, malformed record SKIPPED w/ a
counted total, ⊥ crash, ⊥ a hard schema assert. ONE reader module,
harness-PLUGGABLE (a new harness = a new reader, ⊥ a new tool).
V44: **the ledger is a LOWER BOUND & says so** — V3's honesty rule on
the runtime axis. Load events cannot see system prompt, tool schemas,
`CLAUDE.md`, or prior turns ∴ report `accounted` vs `total` & label the
gap `unaccounted`; NEVER silently attribute the remainder. Where real
`usage` is present the TOTAL is exact & only the ATTRIBUTION is partial —
say which. Every number names its method (V3): `ledger(actual)` ≠
`ledger(bytes/4)`.
V45: **content NEVER enters the ledger & never leaves the box.** Record
path · content hash · count · ts · tool · session; ⊥ file bodies, ⊥
message text, ⊥ env, ⊥ credentials. Transcripts hold the user's & their
customers' content ∴ a telemetry file that copied it is a NEW leak
surface buying nothing — counts answer every question this axis asks. No
egress at all (V35): local files & stdout only; itok ships NO uploader,
NO endpoint, NO opt-out-shaped default.
V46: **runtime verbs keep top/du/strace grammar** (V1) — `trace` = 1
line per load event, chronological (`-n` · `--since` · `--reverse`, the
`git log`/strace shape); `top` = ranked occupancy (`-h` · `-s` ·
`--top N`, the `du`/`top` shape). Both report-only, exit 0 (V5). `log`
stays the PATH-HISTORY verb (V19) & session events are ⊥ routed through
it — `git log` means commits, so overloading it is the V2 near-collision.
NO `blame` verb: per-path attribution is `top -- <path>` ∵ `git blame` is
per-LINE authorship & the almost-match costs more than the flag (V1/V2).
V47: **cache accounting is READ, ⊥ inferred.** `cache_creation` /
`cache_read` come from `usage` ∴ prefix re-billing is OBSERVED, ⊥
modelled. Report it as seen & name it; ⊥ a heuristic "you probably busted
the cache" (that would be a confident guess, V3). Absent fields ⇒ no
cache column, ⊥ a zero (a zero reads as a measurement).
V48: **calibration on CLEAN SAMPLES only.** The estimate-vs-actual
correction factor is derived ONLY from turns where exactly ONE load event
explains the `usage` delta; noisy turns DISCARDED & the surviving `n`
reported beside the factor. A factor from dirty samples is a confident
lie (V3). The factor is REPORTED, ⊥ silently folded into the estimators —
the ladder's rungs stay honest & unmodified (V4). Applying it is opt-in &
LABELLED as derived: `~186k itok (bytes/4 ×1.12 cal:n=340)`.
V49: **`cap` = token-unit filter, visibly ⊥ `head`.** stdin→stdout, pipe
shape (V1). `head`/`tail` truncate SILENTLY by bytes/lines; `cap`
truncates by TOKENS & ANNOUNCES the elision with a machine-parsable
footer ∴ it takes a DIFFERENT NAME, ⊥ `head -t` — an almost-`head` is
V2's expensive failure. Usable with no agent & no hook: `cmd | itok cap
10k` is the whole product of its rung (V42).
V50: **reduction ladder mirrors the precision ladder (V4)** — ordered
LOSSLESS → LOSSIEST, every applied rung NAMED in the footer: `strip`
(ansi · trailing ws) | `dedup` (repeat lines `×N`) | `elide` (base64 ·
minified · lockfile bodies) | `outline` (code → signatures, bodies
dropped) | `cap` (hard truncate). Applied IN ORDER, stopping the moment
the budget is met ∴ the CHEAPEST SUFFICIENT rung wins & the reader is
told exactly which ran. Lossless rungs are default-safe; lossy rungs are
opt-in per policy. NEVER reorder lossy-first — silent structural loss is
the failure this ladder exists to avoid.
V51: **truncation is RESUMABLE & IDEMPOTENT.** The elision footer
carries the resume selector (offset · line range · omitted count) so the
next read CONTINUES, ⊥ restarts at 0. An un-resumable cap costs more than
it saves (the reader re-fetches the whole file). Re-running the same cap
on the same input yields the same cut (V5's determinism, on the filter).
V52: **`guard` is an ADAPTER, ⊥ a daemon.** Shape: harness hook JSON on
stdin → decision JSON on stdout, ONE process per call, no server, no
background thread, no state beyond an append-only session file.
Harness-specific mapping lives in ONE module (V43's pluggability); every
other module is harness-agnostic. ∴ a new agent harness is a new adapter,
⊥ a fork of the tool.
V53: **the gate set is CLOSED & every gate is OPT-IN** — extends V5.
Gates = `check` (committed registry) · `--budget` (inline one-shot) ·
`guard` (runtime policy). Nothing else gates, ever. Enforcement never
self-enables: no `.context-policy` & no installed hook ⇒ itok is exactly
as report-only as it is today. `guard` obeys V5's determinism for V5's
reason — its decision pins a fixed tier, ∵ a gate that varies per run is
⊥ a gate — & takes the CONSERVATIVE estimate (V36), ⊥ the exact count.
V54: **fuse is GRADUATED, w/ hysteresis & a MANDATORY escape hatch.** A
binary deny costs more than the waste (retry loops burn turns) ∴ tiers by
occupancy: `observe` (ledger only) → `warn` (stderr; the agent reads it &
self-corrects) → `cap` (elide, resumable V51) → `deny` (& NAME the
cheaper alternative: `itok fit`, a line range, `rg -m`). Plus a RATE fuse
over a sliding window (N tokens in M calls = runaway `cat huge.log` /
grep into `node_modules`). A trip is STICKY w/ hysteresis ∴ ⊥ flapping on
the boundary. There is ALWAYS an override — a trapped agent is worse than
a fat one.
V55: **the fuse is judged by its OWN telemetry** — V42's other half.
Every trip · cap · override is a ledger event ∴ override-rate &
false-positive rate are MEASURABLE. A high override rate means BAD
POLICY, ⊥ a bad agent, & is the signal to retune. Enforcement that cannot
be measured cannot be tuned ∴ ⊥ shipped.
V56: **pins are ABSOLUTE.** Policy MAY pin paths that are never capped,
elided, or denied (`CLAUDE.md`, `SPEC.md`, the law files). A guard that
elides the rules it guards under is self-defeating. A pin overrides every
tier, including a tripped fuse.
V57: **`.context-policy` is the runtime registry, opt-in like
`.context-limits` (V10).** Per-glob & per-tool budgets · pins (V56) ·
fuse tiers (V54). Absent ⇒ NO enforcement (V53). Reuses the one decimal
unit grammar (V18) & the glob semantics of the existing registries — a
third config file, ⊥ a third config LANGUAGE.
V58: **in-flight MITM proxy DEFERRED (⊥ rejected).** An
`ANTHROPIC_BASE_URL` shim would see EVERYTHING billed & could enforce
hard — but it sits in the CREDENTIAL path & the STREAMING path: it
forwards an API key, buffers SSE, & becomes a new failure mode on EVERY
request. The transcript reader (V43) yields ~the same numbers post-hoc at
ZERO risk, & the hook adapter (V52) yields enforcement at request
granularity ∴ the risky rung buys little. Trigger to revisit: a measured
need for a signal only in-flight interception can give. If ever built:
own opt-in feature (V23), NEVER default, ⊥ read/store/log credentials
(V45), & ! shout what it intercepts. Considered & deferred, escape hatch
recorded (like V21/V24/V25).
V59: **runtime data verbs report AGGREGATES, ⊥ verdicts** — V19's rule,
carried over. `trace`/`top` may compute re-read waste (same blob loaded
N× = N× billed), stale bytes (occupancy × turns-since-touched), dup &
cache columns — those are ARITHMETIC. "This is unhealthy, do X" is
JUDGMENT & belongs to `doctor` if & when it earns a session target (V17's
thin-composer boundary still binds: doctor ⊥ grows tentacles).
V60: **fan-out = N windows, ⊥ one.** Subagent / sidechain sessions each
own a SEPARATE context ∴ the ledger keys by session & rolls up to the
parent. A 12-agent fan-out is 12 windows & its true cost is the SUM —
invisible in any single window, which is exactly why it needs reporting.
V61: **money is a RENDERING, ⊥ a source.** `--cost` multiplies counts by
a rate read from `.context-models` (an optional per-model column); itok
bundles NO price list ∵ prices change & vary by contract ∴ a built-in
would go stale & become a confident lie (V3). Missing rate ⇒ NO money
column, ⊥ a guessed one (V11's unknown-model rule). Cache-read tokens
bill at a different rate than fresh ones ∴ `--cost` ! use the cache
split (V47) or omit the column entirely.
V62: **the flake sits at the REPO ROOT & the dev shell PROVIDES `itok`.**
Root, ⊥ a subdirectory, for one hard reason: a flake's source root is the
directory it lives in ∴ a flake under `.uow/` can ! see `Cargo.toml` |
`src/` & can therefore never build the crate — no `packages.default`, no
`nix build`, no `nix run`. Root also makes it the shape every Rust
project already uses (V1) & the shape `nix flake` commands assume.
PROVIDES: itok dogfoods itself (V15) ∴ its own shell ! hand you the
toolchain and then make you type `cargo run --` — the tool is on PATH.
The shell's `itok` is a SHIM (`exec cargo run -q --manifest-path
"$ITOK_MANIFEST" --bin itok -- "$@"`), ⊥ a package in the shell closure:
a package would build the crate to ENTER the shell ∴ one compile error
locks you out of the shell you need to fix it, & a pinned package is
STALE against the working tree — the opposite of what a dev shell is for.
`cargo run` no-ops on a fresh build ∴ the shim costs ~nothing & is always
current. `ITOK_MANIFEST` is resolved AT ENTRY from `$PWD` (V37: derive,
never hardcode a layout) so it works in-repo & extracted alike;
`ITOK_PROFILE=release` / `ITOK_FEATURES` are the escape hatches. The dev
files are `exclude`d from the published `.crate` (V39): consumer-
meaningless. `packages.*` BUILT (T58): `default` (dummy+bpe),
`itok-minimal` (`--no-default-features` — the zero-dep core, so V23/V13's
claim is a BUILD, ⊥ a promise), `itok-ollama` (+ the LAN rung). Rests on
a COMMITTED `Cargo.lock`: flakes read git-TRACKED files only, & a
sandboxed build cannot resolve versions off the network, so
`cargoLock.lockFile` is what vendors them offline. `doCheck = false` ∵ the
suite shells to git for `HEAD~n` (V33's `gitref`) & a store source has NO
`.git` — that absence is exactly what makes the build reproducible ∴ the
suite runs in the dev shell & CI, where history exists (V37/B3 seen from
the other side). `version` is READ from `Cargo.toml` ∴ one number, ⊥ two
that can disagree.
V64: **ONE gate definition, many callers — eliminate the second copy, ⊥
freeze it.** `hk.pkl` holds the OPS (which command, which flags, which
phase, which order, the coverage floor); the workflow holds ORCHESTRATION
ONLY (runner, toolchain, cache, full history) ∵ that half has no local
equivalent to disagree with. ∴ one definition is reached 3 ways —
`pre-commit`, `pre-push`, `hk check` in CI — & the ops CANNOT drift,
having only one copy. Contrast V40, which had to FREEZE a second
rendering ∵ `--help` text must live in Rust; here both callers can read
the same file, & elimination beats policing (a guard detects drift only
AFTER someone writes it). B4 is the cost of the alternative: a stale `99`
floor sitting in one declaration beside CI's corrected `98`, both looking
authoritative.
V65: **the gate of record stays RUSTUP-REPRODUCIBLE — hk RUNS the ops, ⊥
hides them.** Every step is a plain cargo command a human can paste with
nothing but rustup ∴ the hook manager is a CONVENIENCE, ⊥ a dependency of
the crate (V13/V31). A contributor without hk loses scheduling, never the
ability to reproduce a verdict. Bars, by construction: no step is a
script only hk can call, no verdict depends on hk's own logic.
V66: **the gate's schema is VENDORED, ⊥ fetched at eval** — V12's
reasoning, on config instead of vocab. hk's documented form amends a
`package://` URL fetched over the network at eval time; `pkl/Config.pkl`
ships in-tree & `hk.pkl` amends the local path (upstream's own repo does
the same). Offline-first is the default BUILD, ⊥ merely the default flag
(V34). Re-vendored verbatim on an hk upgrade — the pin in the dev shell,
the vendored schema & CI's `HK_VERSION` are ONE version, moved together.
V67: **cargo steps SERIALIZE by `depends`; parallelism is for the rest.**
Cargo takes a lock on the target directory ∴ two cargo jobs launched
concurrently do ⊥ run concurrently — the second blocks on "Blocking
waiting for file lock on build directory", which READS AS A HANG. The
chain makes the serialization explicit & intended, ⊥ emergent, & leaves
hk free to run any future non-cargo check in parallel. Separate
`CARGO_TARGET_DIR` per step is the escape hatch & is ⊥ worth it (full
recompiles, multiplied disk).
V68: **hermeticity is PROVEN by running parallel, ⊥ argued from
inspection.** `--test-threads=1` is a DIAGNOSTIC (it localizes a race), ⊥
a fix (it hides one) ∴ a suite that needs it has a bug, & the bug is the
deliverable. Reading a test for per-process temp dirs & ephemeral ports
establishes NOTHING — B5 passed that reading & still raced. The gate runs
the suite parallel; a flake found there is fixed at the source, never
suppressed by serializing the axis.
V69: **a registry no runner reads is a LIE, ⊥ a guard.** `.file-limits`
traveled with the crate (V27) but NOTHING standalone enforced it ∴ its
ceilings were hand-maintained & obeyed by no one — it read as law while
gating nothing. Same failure as V28's dead declaration, one level down:
the danger is ⊥ the missing check, it is the FALSE ASSURANCE that a
committed registry provides. ∴ dropped here; file-size ceilings stay a
MONOREPO guard, where a runner exists. Re-add standalone ONLY together w/
a checker wired into the gate (V64) — the registry & its caller land in
the same commit, ⊥ apart.
V70: **version = MATURITY OF THE GUARANTEE, ⊥ feature count.** Each minor
answers ONE question — *what can you rely on at this tag?*: `0.1` builds
reproducibly on any box (`nix build`) · `0.2` cannot regress locally (the
gate runs in git hooks) · `0.4` KNOWS what a context costs (M4 telemetry)
· `0.6` can ACT on it (M5-M7) · `0.7` PUBLIC, featureset frozen, CI
present but unproven · `0.8` public gate trustworthy (matrix, green
streak, release automation) · `1.0` CONTRACT frozen. ODD minors
(`0.3`/`0.5`/`0.9`) are RESERVED for fix/backprop releases ∴ a bugfix
never borrows a milestone's number. The ladder ENFORCES V42
structurally: telemetry is `0.4`, enforcement is `0.6` ∴
observe-before-enforce stops being a rule to remember & becomes a number
you cannot skip. `0.7` freezes the SPEC's PLANNED featureset; `1.0`
freezes the CONTRACT (CLI surface + json, V9) — additions between them
are ALLOWED ∵ they come from public feedback, which is the POINT of
exposing at `0.7`. Pre-1.0 SemVer (minor MAY break) is HONEST here ∵ each
rung IS a behavior change. crates.io is IMMUTABLE (yank HIDES, ⊥ deletes)
∴ the first public artifact is `0.7.0-rc.1` — cargo ⊥ selects a
pre-release by default — so the pipeline is proven before a permanent
number is spent.
V71: **a passing gate says NOTHING; a failing one says what to DO.** Unix
philosophy, applied to the gate. SUCCESS = SILENCE (`HK_HIDE_WHEN_DONE`)
∵ output that always appears is output nobody reads ∴ the one real
failure hides in the noise everyone learned to scroll past. FAILURE =
LOUD & ACTIONABLE: the tool's own diagnostic, + a one-line remediation
pointer on the steps whose fix is ⊥ obvious from the command that failed
(`itok`/`ollama`/`no-default-features`/`package`/`coverage`), + a
playbook (`AGENTS.md`) written for agents & humans alike. FAIL-FAST is
LOCAL, ⊥ universal: locally the loop is cheap ∴ the FIRST failure is the
most useful thing to surface & `depends` orders steps cheapest-first so
it arrives fast; in CI a round-trip costs minutes ∴ `--no-fail-fast` — a
COMPLETE list beats an early one. BYPASS is ⊥ a fix: `--no-verify` ·
`HK=0` · lowering a threshold · `#[allow]` all SHIP THE DEFECT with the
alarm switched off. If the CHECK itself is wrong, that is a SPEC change —
made deliberately, in its own commit, ⊥ silently at the failure site.
V72: **a gate step MAY need a binary; the VERDICT may ⊥ need one.** The
`hk util` family is native to the runner ∴ free everywhere. A second tier
(`typos`·`actionlint`·`taplo`·`shellcheck`) needs a real binary, & that
is allowed — V65 binds the OPS a verdict rests on (cargo, rustup-only),
⊥ every auxiliary lint. Rule: the tool is PINNED in the dev shell &
installed at a PINNED version in CI, ⊥ floating ∴ the two runners agree
(V64). `actionlint` earns its place specially: the workflow is the ONE
file NO local run exercises (V64 keeps orchestration there) ∴ without it,
a mistake in the workflow is found only by pushing.
V73: **an exemption NAMES what is deliberate; it is ⊥ a suppression.**
`.typos.toml` exempts `esimate` ∵ it is the INPUT to the typo-suggestion
tests (V6) — the mistake ! be spelled correctly in both the unit test &
the README example — & exempts `pkl/` + `tests/fixtures/` ∵ one is
vendored VERBATIM (V66) & the other is a RECORDING (V38): "fixing" either
silently forks it. Every exemption carries its REASON in the config file,
⊥ in a commit message nobody reads at the failure site. A suppressed
finding w/o a named reason is indistinguishable from a bug (V71's
no-bypass rule, applied to config).
V74: **a hook DEGRADES when its runner is absent; it ⊥ BLOCKS.** A gate
exists to stop bad commits, ⊥ to stop git. When the runner is missing —
outside the dev shell, a fresh clone, a container — the hook prints ONE
line to stderr & exits 0: LOUD about being skipped, ∵ a SILENTLY skipped
gate is indistinguishable from a passing one (V71). Hard-failing instead
makes the repo unusable for anyone who ⊥ installed the toolchain, which
is the wrong failure mode for a CONVENIENCE — the gate of record is CI +
`hk check` (V65), ⊥ the hook. ∴ the hook command is written BY HAND, ⊥ by
`hk install`, whose command assumes its own binary is on PATH (B6).

## §T TASKS

| id | scope | tasks | done-when |
|----|-------|-------|-----------|
| M1 | offline core — estimate + report | T1, T2, T3, T4, T12 | `itok estimate`/`itok e` green on a real tree, json stable, `--budget` gates |
| M2 | full ladder + gate + extract | T5, T7, T8, T9, T10, T11, T13, T14, T15, T16, T17, T18, T19, T20, T21, T22, T23, T24 | `--bpe`/`--ollama`/`diff`/`check`/`doctor`/`log`/`fit` done, subtree-split rehearsed |
| M3 | publish — crates.io-ready, public CI | T25, T26, T27, T28, T29 | `cargo publish --dry-run` clean, public CI green, no origin-repo ref shipped, docs regenerate, `itok` installs |
| M4 | runtime introspection — read-only ledger (V41, V42) | T30, T31, T32, T33, T34, T35, T36, T37, T38, T49 | `itok trace`/`itok top` green on itok's OWN sessions, accounted-vs-unaccounted stated, calibration factor + `n` reported, zero interception |
| M5 | reduction — the standalone pipe filter | T39, T40, T41 | `cmd \| itok cap 10k` useful w/ no agent & no hook, every applied rung named in the footer, cut is resumable & deterministic |
| M6 | enforcement — policy · guard · fuse | T42, T43, T44, T45, T46 | hook adapter decides from `.context-policy`, fuse graduated + always overridable, every trip/cap/override lands in the ledger |
| M9 | public exposure — ships as `0.7.0` | T59 | repo public, `0.7.0-rc.1` published & installed from crates.io, then `0.7.0`; badges resolve |
| M10 | CI hardening — ships as `0.8.0` | T60 | matrix (linux + macos-arm) green over a streak, release automation, MSRV axis |
| M11 | closure — ships as `1.0.0` | T61 | public feedback folded; CLI surface + json contract frozen |
| M8 | local guardrails — ONE gate definition, run by hk | T54, T55, T56 | `hk.pkl` is the only place an op is written; pre-commit/pre-push/CI all reach it; the suite is green run PARALLEL |
| M7 | runtime CI — replay & regression | T47, T48 | a recorded ledger replays deterministically offline ∴ policy A/B w/o an agent; a run over session budget fails CI |

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
T19|x|unit declaration (ops×globs×phases, resolved-path bins) — MONOREPO-ONLY; ⊥ shipped in the standalone repo, where `ci.yml` is the runner (superseded by T51)|V28,V29
T20|x|pinned detachable toolchain: `flake.nix`+`flake.lock`+`.envrc` — relocated to the repo ROOT by T50|V28,V62
T21|x|itok baselines at crates/itok/ root: `lexicon.txt`·`coverage-baseline.json` (`.file-limits` dropped by T57 — unenforceable standalone)|V27,V69
T22|x|`gitref` primitive: token cost of a file AT a commit (`git cat-file` → dummy/bpe), shared by diff/show/log|V33
T23|x|`show` verb: one commit's per-file delta (default HEAD), `-- path`, `<commit>:<path>` blob|V32,V33
T24|x|cassette-replay the ollama backend: `vcr-cassette` dev-dep + ureq replay stub + fixtures (`/api/tags`·`/api/show`·`/api/generate`); live smoke → `#[ignore]`; `ollama` CI axis in `.uow`+hook; fold into `src/ollama/`|V38,V22,V23
T25|x|metadata: `Cargo.toml` version `0.8.0` + `repository`·`homepage`·`documentation`·`readme`·`keywords`·`categories`·`rust-version`, `exclude` non-consumer files|V39,V13
T26|x|public-clean provenance: scrub SPEC + src comments of origin-repo names & cross-repo invariant pointers (fold inline), header un-names the workspace, `exclude=[".uow"]`|V26,V39
T27|x|bare-rust `.github/workflows/ci.yml`: fmt·clippy·test·`--features ollama`·`llvm-cov --fail-under-lines 98`, `fetch-depth:0`, no nix/uow|V39,V31,V38
T28|x|assemble README = hand narrative + the `itok docs` reference block + badges (post-ORG); `CHANGELOG.md`; crate rustdoc for docs.rs|V39,V40,V15
T29|x|`itok docs` verb: ONE command registry (verb·synopsis·flags·exit) renders `--help` + a markdown reference; read-only to stdout; guard diffs it vs README ∴ docs can't rot|V40,V6,V9
T30|.|session reader module: harness-PLUGGABLE, defensive JSONL parse → load events; unknown fields ignored, malformed records skipped & COUNTED; ⊥ writes to the transcript|V43,V45
T31|.|`trace` verb: 1 line/load event, chronological, `-n`·`--since`·`--reverse`·json|V46,V9,V59
T32|.|`top` verb: ranked occupancy, `-h`·`-s`·`--top N`, dup + stale columns, `-- <path>` per-path attribution (⊥ a `blame` verb)|V46,V59
T33|.|accounted-vs-unaccounted split; method label on every runtime number (`ledger(actual)` ≠ `ledger(bytes/4)`)|V44,V3
T34|.|cache columns read from `usage` (`cache_creation`/`cache_read`); fields absent ⇒ NO column, ⊥ a zero|V47
T35|.|fan-out rollup: ledger keyed by session, sidechain/subagent sessions rolled to parent, SUM reported|V60
T36|.|`calibrate` verb: factor from single-load turns ONLY, discards counted, `n` reported; application opt-in & labelled `×1.12 cal:n=340`|V48,V4
T37|.|DOGFOOD: run `trace`/`top`/`calibrate` over itok's OWN dev sessions; the measured waste (re-read, stale, cache-bust) sets M6's default thresholds, ⊥ guessed ones|V42,V15
T38|.|`--cost` rendering: rate column in `.context-models`, cache-split aware; missing rate ⇒ no column|V61,V11
T39|.|`cap` verb: stdin→stdout token filter + ANNOUNCED elision footer (human/json) carrying a resume selector|V49,V51
T40|.|reduction ladder rungs `strip`·`dedup`·`elide`·`outline`: applied lossless→lossiest, stop at budget, applied rungs named in the footer; lossy rungs opt-in|V50
T41|.|`cap` determinism + resume round-trip tests: same input ⇒ same cut; footer selector actually continues (proptest the tail)|V51,V5
T42|.|`.context-policy` parser: per-glob & per-tool budgets · pins · fuse tiers; reuses the V18 unit grammar & existing glob semantics; absent ⇒ no enforcement|V57,V18,V53
T43|.|`guard` adapter: hook JSON stdin → decision JSON stdout, ONE process per call, no daemon; harness mapping isolated to one module; decision in JSON ⊥ exit code|V52,V53
T44|.|fuse state machine: occupancy tiers observe→warn→cap→deny (deny NAMES a cheaper alternative) + sliding-window RATE fuse + sticky hysteresis + override hatch|V54
T45|.|pins honored absolutely — above every tier & above a tripped fuse|V56
T46|.|fuse telemetry: trip · cap · override are ledger events; `top` reports override-rate as the policy-quality signal|V55
T47|.|ledger REPLAY: recorded events × a policy, offline & deterministic ⇒ policy A/B with no agent & no network|V53,V5
T48|.|session-cost regression gate: a recorded run over its input-token budget fails CI (the `--budget` shape, on a session)|V53,V16
T54|x|adopt hk as the gate runner: vendor `pkl/Config.pkl` (⊥ `package://`), `hk.pkl` w/ fast set + `all` amending it, `depends` chain over the cargo steps, `stash = "git"` on pre-commit, dev shell provides hk pinned to the vendored schema's version|V64,V66,V67
T55|x|`ci.yml` = orchestration ONLY, delegating to `hk check --all --check`; `tests/ci.rs` retargeted from freezing a second copy to freezing the SINGLE definition (ops in hk.pkl, workflow restates none, schema vendored)|V64,V65
T56|x|fix the cassette stub's request drain (B5): read to `Content-Length` before replying, `Connection: close`; verified 8/8 parallel runs green where it was 2/3 failing|V68,V38
T58|x|`packages.default`/`itok-minimal`/`itok-ollama` via `buildRustPackage`+`cargoLock.lockFile`; version read from `Cargo.toml`; `doCheck = false` (no `.git` in the store); `nix build`/`nix run` verified on aarch64-darwin for all three|V62,V23
T57|x|drop `.file-limits`: no standalone runner reads it ∴ hand-maintained ceilings gating nothing; refs scrubbed from SPEC/CONTRIBUTING/`Cargo.toml` exclude; file-size limits remain a monorepo guard|V69,V28
T59|.|public exposure: create the GitHub repo, publish `0.7.0-rc.1` FIRST (immutability -- prove the pipeline before spending a permanent number), install it from crates.io, then `0.7.0`|V70,V39
T60|.|CI hardening: platform matrix (ubuntu + macos-arm), MSRV `1.82` axis, release automation, a green streak before `0.8.0`|V70,V39
T61|.|closure: fold public feedback, freeze the CLI surface & the json contract, `1.0.0`|V70,V9
T62|x|gate UX: silent on success (`HK_HIDE_WHEN_DONE`), explicit `fail_fast` locally & `--no-fail-fast` in CI, per-step remediation hints on the non-obvious failures, `AGENTS.md` playbook + CONTRIBUTING pointer|V71,V65
T63|x|external-tool lints: `typos`·`actionlint`·`taplo`·`shellcheck` pinned in the dev shell + at pinned versions in CI (`install-action` for 3, pinned release download for actionlint); `.typos.toml` exemptions named|V72,V73
T64|x|dev shell installs hooks on entry, written BY HAND w/ a runner-missing guard (B6), skipped when a global install exists ∵ git aggregates `hook.*.command` across scopes ⇒ double fire|V71,V74
T65|x|`packages.*` src narrowed via `lib.fileset` to `src/`+`Cargo.toml`+`Cargo.lock` ∴ a doc edit no longer invalidates the build|V62
T49|.|SPEC compaction debt (M3 closed, now DUE): compact §V/§B prose; one-file rule holds — more sections, ⊥ more files|V15
T50|x|flake to repo ROOT (`git mv` out of the unit dir) + dev shell PROVIDES `itok` via a `cargo run` shim; `ITOK_MANIFEST` resolved at entry, `ITOK_PROFILE`/`ITOK_FEATURES` hatches; dev files `exclude`d from the `.crate`|V62,V15,V39
T51|x|drop the unit declaration from the standalone repo; `ci.yml` carries all 5 ops at >= their old strength (clippy gains `--all-features -- -D warnings`, coverage keeps the corrected 98) & records what was deliberately ⊥ carried (`--test-threads=1`, the phase split)|V28,V29,V31
T52|x|`.gitignore` (`/target`, `/.direnv`): cargo packages untracked-not-ignored files ∴ `.direnv/` (5.5MB of vendored nixpkgs source) was landing in `cargo package --list` ⇒ would ship in the `.crate`|V39,V13
T53|x|track `Cargo.lock`: a BIN crate pins its own deps, & nix flakes read git-TRACKED files only ∴ an untracked lock is invisible to a `packages.default` build|V62,V39

## §B BUGS

id|date|cause|fix
B1|2026-07-24|extraction: `rustfmt.toml`/`clippy.toml` root-only ⇒ standalone `cargo fmt --check` reformats (default width 100 vs 80); traveling gate ⊥ reproduces verdict|V27
B2|2026-07-24|extraction: git-command tests use `CARGO_MANIFEST_DIR.parent().parent()` as repo root + hardcode `crates/itok/…` paths ⇒ standalone `cargo nextest` fails|V37
B3|2026-07-24|`diff --budget` test asserted a breach on live `HEAD~1..HEAD`, presuming a substantial last commit; a near-zero-delta ceiling bump as HEAD gave no breach ⇒ exit 0 ≠ 1, blocking every commit until HEAD grew|V37
B6|2026-07-25|dev-shell auto-install wrote `hk install`'s command, which assumes `hk` on PATH. Outside the shell: `hk: command not found` ⇒ hook exit nonzero ⇒ EVERY git commit in the repo blocked, ⊥ merely ungated. Caught within one minute, by the very next commit failing|V74
B5|2026-07-25|cassette replay stub read the request with ONE `read()`; TCP segments the only POST (`/api/generate`) so the body can arrive second ⇒ reply-then-close raced the client's write ⇒ `itok: ollama read ...: Invalid argument (os error 22)`. Flaky 2/3 runs parallel, 3/3 green serial ∴ the deleted unit declaration's `--test-threads=1` had been MASKING it, & the claim that the suite was hermetic (argued from reading temp-dir/port handling) was wrong. Surfaced the first time the new gate ran the ollama axis|V68
B4|2026-07-25|ci.yml `--fail-under-lines 99` copied the monorepo WORKSPACE total; itok ALONE is 98.03% (siblings inflate the aggregate) ⇒ the standalone gate fails coverage. Caught by the T359 proving ground. Floor set to itok's own 98|V31
