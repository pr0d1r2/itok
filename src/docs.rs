//! The single command registry (V40): one table of verbs -> synopsis +
//! blurb, rendered two ways so `--help` and `itok docs` can never disagree.
//! `usage()` is the terse `--help` -- synopsis + one line per verb;
//! `markdown()` adds the reference paragraph and is the block a
//! guard freezes into README (the freeze test below). Add a verb HERE and
//! both views update; a verb without a row is caught by `registry_names_
//! every_verb`. Generated = REFERENCE only -- the narrative (intro, the
//! precision ladder, the mermaid) stays hand-written ABOVE the block.

/// One command's docs: its name, argument synopsis, a ONE-LINE blurb, and
/// the reference paragraph.
///
/// TWO fields for the text because two callers want different lengths, and
/// one field serving both is how `rate`'s grew to 3,977 characters -- 39%
/// of everything `--help` printed. `--help` takes the blurb; `itok docs`
/// and README take both. `blurb_is_one_line` is what keeps it that way.
///
/// The reference states the SURFACE and its refusals. It does not recite
/// the invariant behind them: that has a home in `SPEC.md`, and a second
/// copy is the one that goes stale (V64).
struct Command {
    name: &'static str,
    synopsis: &'static str,
    blurb: &'static str,
    reference: &'static str,
}

const COMMANDS: &[Command] = &[
    Command {
        name: "estimate",
        synopsis: "estimate [-s] [-h] [--top N] [--budget N] [--bpe] [--ollama[=HOSTS]] [--format human|json] [-C dir] [paths...]",
        blurb: "Token cost of files, git-tracked by default. `--budget N` turns it into a gate.",
        reference: "`--bpe` swaps bytes/4 for a real tokenizer (o200k); `--ollama` gets an exact count from a local model's own tokenizer, and a bare host needs the `=` form (`--ollama=$OLLAMA_HOST`). The two tiers agree on DELTAS even where they differ on absolutes, so `--bpe` answers which of two texts costs less without a fleet.",
    },
    Command {
        name: "doctor",
        synopsis: "doctor [--session [<id>]] [--model X[,Y...]] [--window N] [--ollama[=HOSTS]] [-h] [--format human|json] [-C dir] [paths...]",
        blurb: "Advisory health check over a fileset, or over a running session with `--session`. Reports and suggests; never gates.",
        reference: "Over a fileset: fit-to-window, budget balance, noise ratio, estimate confidence. `--model X` resolves an encoding via `.context-models`; `--model a,b` narrows an `--ollama` fleet, and one unresolvable name fails the call.\n\n`--session [<id>]` retargets the whole verb at a running context. It prints `headroom`'s row -- window, used, avail, use%, the rate triple, `~turns left` -- then projects each item's occupancy across the turns remaining, biggest first. A transcript carrying a compaction boundary already dropped items, so its projection is labelled an upper bound. No capacity means no `~turns left` and no projection, and the report says which absence it is. A second positional is a usage error: this form has no fileset.\n\nWhen `use%` crosses the line `fit` warns at, and only then, it ends with advice: the two levers measured to help, and the one it rejects by name.",
    },
    Command {
        name: "diff",
        synopsis: "diff [<A> <B> | <A>..<B> | <ref>] [--staged] [--exit-code] [--budget N] [--bpe] [-- <path>]",
        blurb: "Token delta between two points, git-diff-shaped. Default is the working tree against HEAD.",
        reference: "`--exit-code` or `--budget N` makes it a gate.",
    },
    Command {
        name: "show",
        synopsis: "show [<commit>] [-- <path>] | show <commit>:<path>",
        blurb: "One commit's per-file token delta. Default HEAD.",
        reference: "The `<commit>:<path>` form reports a single blob's cost at that ref.",
    },
    Command {
        name: "log",
        synopsis: "log <path> [<A>..<B>] [-n N] [--since D] [--reverse] [--bpe] [--format human|json]",
        blurb: "A path's token cost and delta across every commit that touched it -- the creep curve.",
        reference: "Report-only, git-log-shaped.",
    },
    Command {
        name: "check",
        synopsis: "check [-C dir] [--format human|json]",
        blurb: "Gate registered paths against `.context-limits`. Exit 1 on any breach.",
        reference: "The tokenizer is pinned (`--bpe`), so the verdict is deterministic across machines.",
    },
    Command {
        name: "guard",
        synopsis: "guard",
        blurb: "Hook adapter: one harness payload on stdin, one decision on stdout.",
        reference: "Decides from `.context-policy` -- per-glob and per-tool budgets, with pins allowed absolutely. No policy file means allow, silently, so enforcement never self-enables. The decision is in the JSON, never in the exit code.",
    },
    Command {
        name: "fit",
        synopsis: "fit --window N [--by size] [--bpe] [--format human|json] [-C dir] [paths...]",
        blurb: "Greedy subset of files that fits a token window; emits a pipeable path list.",
        reference: "Git-tracked by default. `itok fit --window 200k src/ | xargs cat` builds a context bundle under budget.",
    },
    Command {
        name: "trace",
        synopsis: "trace [<session>] [-n N] [--since D] [--reverse] [--format human|json]",
        blurb: "Runtime load events for a session, one line each, chronologically.",
        reference: "What entered the context, when, and how big. Defaults to the newest transcript for the working directory. Report-only, and per-event sizes are estimates (`bytes/4`): no content is stored, so there is nothing to tokenize.",
    },
    Command {
        name: "top",
        synopsis: "top [<session>] [-- <path>] [-h] [-s] [--top N] [--format human|json]",
        blurb: "Ranked context occupancy for a session, `du`-shaped. Report-only.",
        reference: "How much each thing cost, how many times it was loaded, how many turns since, and how much cache re-billing it has `carried` since it entered. Whether that product is exact is read off the session: a transcript carrying a compaction boundary had items leave, so its `carried` is an upper bound and the report names when.\n\n`-- <path>` narrows to one path's loads. Ends with the accounted-vs-unaccounted split, each number naming its method.",
    },
    Command {
        name: "headroom",
        synopsis: "headroom [<session>] [--model X] [--window N] [--task N] [-h] [--format human|json] [-C dir]",
        blurb: "`df` for a context: window, used, avail, use%, the growth rate, and `~turns left`. Report-only.",
        reference: "The rate is over the last 10/50/200 TURNS -- context grows per turn, so a per-second rate would be meaningless. Without `--window` or `--model` there is no capacity, so `avail`/`use%`/`turns left` report as absent rather than against a guessed window. `--task N` adds a `tasks left` column: N is the WINDOW a task occupies, not what it bills.",
    },
    Command {
        name: "calibrate",
        synopsis: "calibrate [<session>] [-h] [--format human|json] [-C dir]",
        blurb: "What a session's context actually cost against what itok estimated.",
        reference: "A fixed overhead the transcript cannot see (system prompt + tool schemas) and a scale from `bytes/4` to real tokens, with the error BAND measured on turns the fit never saw, plus `n` -- never a bare factor. Too few turns reports `n` and no factor. The scale absorbs message framing and unrecorded reasoning, so it is derived per session and is not a tokenizer ratio.",
    },
    Command {
        name: "rate",
        synopsis: "rate [<session>] [--statusline] [--color auto|always|never] [--format human|json] [-C dir]",
        blurb: "Pre-formatted badge for a statusline: occupancy, the bill, and growth per hour and per day.",
        reference: "Cell 1 is a LEVEL: the last turn's billed input, coloured as a fill gauge against the point the session is heading for -- `[rate].compact` if declared (one number, or a table keyed by model), else the auto-compact point this session's own transcript recorded, else the model's window. 24-bit where `COLORTERM` allows, three bands otherwise, and the last tenth also takes a `!`. A zero window is never painted: an absent measurement is not an empty context.\n\nCells 3 and 4 divide GROWTH -- the sum of positive window deltas -- not the bill, which is mostly cache re-reads and so measures how long the session has run rather than how full it is getting. Cell 2 keeps the bill under a name that says so; it is what the API was asked to charge, which is neither money nor content.\n\nA fifth cell appears when a red line resolved and the context is growing: time to it at the recent rate, floored and tilde'd, in ACTIVE seconds. Rates divide by active time, each inter-turn gap credited up to 300 seconds.\n\n`--statusline` reads the harness payload on stdin and emits `(itok:...)`; colour defaults to `always` there. `--color` reads `[rate]` thresholds from `itok.toml`; a metric with no threshold gets no colour. 0-1 turns hides the badge, but `--format json` still returns one object with `null` where nothing was measured.",
    },
    Command {
        name: "cap",
        synopsis: "cap [N] [--footer human|json]",
        blurb: "Token-budget filter for a pipe: the longest whole-line prefix that fits N tokens, and it announces the cut.",
        reference: "The footer says what was kept, what was elided, and the line and byte offset to resume from, so the next read continues rather than restarting. The line number is exact on any stream; the byte offset is of the decoded text, so it is for UTF-8 input. `head` truncates silently by lines or bytes; this truncates by tokens and says so. Without N nothing is cut and the footer just reports the cost.",
    },
    Command {
        name: "docs",
        synopsis: "docs",
        blurb: "Print this command reference as markdown -- the source for README's generated block.",
        reference: "Kept in sync by a guard, so the block and this table cannot disagree.",
    },
];

const EXIT: &str = "\
| code | meaning |
|------|---------|
| `0` | ok |
| `1` | budget breach or nonzero delta |
| `2` | usage error |
| `7` | network error (`--ollama`) |";

/// The terse `--help` text, rendered from the registry (V40).
pub(crate) fn usage() -> String {
    let body: String = COMMANDS.iter().map(terse).collect();
    format!(
        "itok -- context-cost estimator\n\n\
         usage: itok <command> [args]\n\ncommands:\n{body}{}",
        tiers()
    )
}

/// Which optional tiers THIS binary actually has.
///
/// The synopses above list every flag itok can offer, and the README block
/// is frozen against `markdown()` so it must stay build-independent (V40).
/// So the build-specific honesty lives here, in `--help` only: without it,
/// a stripped binary prints `unknown flag '--ollama'` immediately above a
/// synopsis advertising `--ollama` (B11e).
fn tiers() -> String {
    let mut on = vec!["bytes/4"];
    if cfg!(feature = "bpe") {
        on.push("--bpe (o200k)");
    }
    if cfg!(feature = "ollama") {
        on.push("--ollama (exact)");
    }
    if cfg!(feature = "session") {
        on.push("runtime verbs (trace/top/headroom)");
    }
    format!("\ntiers in this build: {}\n", on.join(", "))
}

/// `--help`: synopsis and ONE line. It is the thing an agent pays for on
/// every invocation, so it is the thing kept short (V94's entry cost, on
/// our own surface).
fn terse(c: &Command) -> String {
    format!("  {}\n      {}\n", c.synopsis, c.blurb)
}

/// The markdown command reference (`itok docs`), frozen into README (V40).
/// This IS the block between README's `itok docs` markers.
pub(crate) fn markdown() -> String {
    let body: String = COMMANDS.iter().map(section).collect();
    format!(
        "## Commands\n\nEvery verb, its synopsis and what it does. \
         Regenerate with `itok docs`.\n\n{body}## Exit codes\n\n{EXIT}\n"
    )
}

/// The README/`itok docs` entry: both halves, because a reference is read
/// deliberately and once, not on every call.
fn section(c: &Command) -> String {
    format!(
        "### `{}`\n\n```text\n{}\n```\n\n{}\n\n{}\n\n",
        c.name, c.synopsis, c.blurb, c.reference
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Between README's `<!-- BEGIN itok docs -->` / `<!-- END ... -->`
    /// markers, exclusive of the marker lines.
    fn readme_block() -> String {
        let path = crate::testutil::crate_path("README.md");
        let full = std::fs::read_to_string(
            std::path::Path::new(&crate::testutil::repo_root()).join(path),
        )
        .unwrap_or_default();
        let after = full.split_once("<!-- BEGIN itok docs -->\n");
        let inner =
            after.and_then(|(_, r)| r.split_once("<!-- END itok docs -->"));
        inner.map(|(b, _)| b.to_owned()).unwrap_or_default()
    }

    /// The struct doc says "one-line blurb", and now something enforces
    /// it. `rate`'s reached 3,977 characters -- 39% of everything
    /// `--help` printed -- because one field served two audiences and
    /// nothing objected as it grew a sentence per invariant (T107).
    ///
    /// A cap rather than a taste: the reference field is where a long
    /// answer goes, so there is somewhere for the sentence to land.
    #[test]
    fn every_blurb_is_one_line() {
        for c in COMMANDS {
            assert!(
                !c.blurb.contains('\n'),
                "{}: blurb is not one line",
                c.name
            );
            assert!(
                c.blurb.len() <= 160,
                "{}: blurb is {} chars, cap is 160 -- the long half \
                 belongs in `reference`",
                c.name,
                c.blurb.len()
            );
        }
    }

    /// `--help` is paid on every invocation, and this tool exists to
    /// notice costs like that. The budget is the measurement: 623 itok
    /// after T107, against 2,570 before it, so 900 leaves room to add a
    /// verb and fires long before another recitation lands.
    #[test]
    fn help_stays_cheap_to_read() {
        let chars = usage().len();
        assert!(
            chars <= 3_600,
            "`--help` is {chars} chars (~{} itok): the entry cost this \
             tool measures for everyone else",
            chars / 4
        );
    }

    /// The reference is the OTHER half, so it must actually be reached
    /// -- a split that only shortened `--help` would have lost the text.
    #[test]
    fn the_markdown_block_carries_both_halves() {
        let m = markdown();
        for c in COMMANDS {
            assert!(m.contains(c.blurb), "{}: blurb missing", c.name);
            assert!(m.contains(c.reference), "{}: reference missing", c.name);
        }
    }

    #[test]
    fn usage_lists_every_verb() {
        let u = usage();
        assert!(u.contains("usage: itok"));
        for c in COMMANDS {
            assert!(u.contains(c.name), "usage missing {}", c.name);
        }
    }

    #[test]
    fn markdown_lists_every_verb() {
        let m = markdown();
        assert!(m.starts_with("## Commands"));
        assert!(m.contains("## Exit codes"));
        for c in COMMANDS {
            assert!(m.contains(&format!("### `{}`", c.name)), "{}", c.name);
        }
    }

    #[test]
    fn registry_names_every_verb() {
        // Every prefix-inferred verb has a doc row -- adding a verb without
        // documenting it fails here (V40: one registry, no rot).
        for (v, _) in crate::verb::VERBS {
            assert!(
                COMMANDS.iter().any(|c| c.name == *v),
                "verb `{v}` has no docs row"
            );
        }
    }

    /// B11e: `--help` states which tiers this binary HAS, because the
    /// synopses list every flag itok can offer and the README block is
    /// frozen build-independently. Without the line, a stripped binary
    /// advertises what it will then reject.
    #[test]
    fn usage_names_the_tiers_this_build_has() {
        let u = usage();
        assert!(u.contains("tiers in this build:"));
        assert!(u.contains("bytes/4"), "the always-present tier");
        assert_eq!(
            u.contains("--bpe (o200k)"),
            cfg!(feature = "bpe"),
            "the line must track the BUILD, not the synopsis"
        );
    }

    /// The frozen block must NOT vary by feature, or the freeze would
    /// fail under a different feature set (V40).
    #[test]
    fn the_markdown_block_is_build_independent() {
        assert!(!markdown().contains("tiers in this build"));
    }

    #[test]
    fn readme_block_matches_markdown() {
        // The freeze (V40): README's generated block IS `itok docs` output.
        // Drift -- a hand-edit, or a verb added without `itok docs` -- fails
        // here. Regenerate with `itok docs > the block`.
        assert_eq!(
            readme_block().trim(),
            markdown().trim(),
            "README block stale -- regenerate: `itok docs`"
        );
    }
}
