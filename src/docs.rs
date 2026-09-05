//! The single command registry (V40): one table of verbs -> synopsis +
//! blurb, rendered two ways so `--help` and `itok docs` can never disagree.
//! `usage()` is the terse `--help`; `markdown()` is the reference block a
//! guard freezes into README (the freeze test below). Add a verb HERE and
//! both views update; a verb without a row is caught by `registry_names_
//! every_verb`. Generated = REFERENCE only -- the narrative (intro, the
//! precision ladder, the mermaid) stays hand-written ABOVE the block.

/// One command's docs: its name, argument synopsis, and a one-line blurb.
struct Command {
    name: &'static str,
    synopsis: &'static str,
    blurb: &'static str,
}

const COMMANDS: &[Command] = &[
    Command {
        name: "estimate",
        synopsis: "estimate [-s] [-h] [--top N] [--budget N] [--bpe] [--ollama[=HOSTS]] [--format human|json] [-C dir] [paths...]",
        blurb: "Token cost of files, git-tracked by default. `--bpe` swaps bytes/4 for a real tokenizer (o200k); `--ollama` gets an exact count from a local model's own tokenizer; a bare host needs the `=` form (`--ollama=$OLLAMA_HOST`). `--budget N` turns it into a gate.",
    },
    Command {
        name: "doctor",
        synopsis: "doctor [--session [<id>]] [--model X[,Y...]] [--window N] [--ollama[=HOSTS]] [-h] [--format human|json] [-C dir] [paths...]",
        blurb: "Advisory health check: fit-to-window, budget balance, noise ratio, estimate confidence. Reports and suggests; never gates. `--model X` resolves an encoding via `.context-models`, and `--model a,b` narrows an `--ollama` fleet to those models (one unresolvable name fails the call); `--ollama[=HOSTS]` discovers live model windows across a fleet; a bare host needs the `=` form. `--session [<id>]` retargets the whole verb at a running CONTEXT instead of a fileset: it prints `headroom`'s row for that session -- window, used, avail, use%, the rate triple and `~turns left` -- and then projects each item's occupancy forward across the turns remaining, biggest first. That product is the forward half of `top`'s `carried`, and it inherits the same caveat, read off the session's own record: a transcript carrying a compaction boundary already dropped items, so the projection is labelled an upper bound. No capacity means no `~turns left` and therefore no projection at all, and the report says so rather than printing an empty block. Every figure comes from `headroom` or `top`; this form composes them and adds no estimator of its own. A second positional is a usage error: this form has no fileset. When `use%` crosses the line `fit` warns at, and only then, the report ends with advice: the two levers measured to help (cap what enters; end the session or fan out), and the one it rejects by name -- evicting what is already in costs more than it saves, because the prefix is append-only in practice.",
    },
    Command {
        name: "diff",
        synopsis: "diff [<A> <B> | <A>..<B> | <ref>] [--staged] [--exit-code] [--budget N] [--bpe] [-- <path>]",
        blurb: "Token delta between two points (default: working tree vs HEAD), git-diff-shaped. `--exit-code` or `--budget N` makes it a gate.",
    },
    Command {
        name: "show",
        synopsis: "show [<commit>] [-- <path>] | show <commit>:<path>",
        blurb: "One commit's per-file token delta (default HEAD). The `<commit>:<path>` form reports a single blob's cost at that ref.",
    },
    Command {
        name: "log",
        synopsis: "log <path> [<A>..<B>] [-n N] [--since D] [--reverse] [--bpe] [--format human|json]",
        blurb: "A path's token cost and delta across every commit that touched it -- the creep curve. Report-only, git-log-shaped.",
    },
    Command {
        name: "check",
        synopsis: "check [-C dir] [--format human|json]",
        blurb: "Gate registered paths against `.context-limits` (pinned `--bpe`, so the verdict is deterministic). Exit 1 on any breach.",
    },
    Command {
        name: "guard",
        synopsis: "guard",
        blurb: "Hook adapter for a harness: reads one hook payload on stdin, writes a decision on stdout, one process per call. Decides from `.context-policy` -- per-glob and per-tool budgets, with pins allowed absolutely. No policy file means allow, silently, so enforcement never self-enables. The decision is in the JSON, never in the exit code.",
    },
    Command {
        name: "fit",
        synopsis: "fit --window N [--by size] [--bpe] [--format human|json] [-C dir] [paths...]",
        blurb: "Greedy subset of files that fits a token window; emits a pipeable path list (git-tracked by default). `itok fit --window 200k src/ | xargs cat` builds a context bundle under budget.",
    },
    Command {
        name: "trace",
        synopsis: "trace [<session>] [-n N] [--since D] [--reverse] [--format human|json]",
        blurb: "Runtime load events for a session, one line each, chronologically -- what entered the context, when, and how big. Defaults to the newest transcript for the working directory. Report-only. Per-event sizes are estimates (`bytes/4`): no content is stored, so there is nothing to tokenize.",
    },
    Command {
        name: "top",
        synopsis: "top [<session>] [-- <path>] [-h] [-s] [--top N] [--format human|json]",
        blurb: "Ranked context occupancy for a session, `du`-shaped: how much each thing cost, how many times it was loaded, how many turns have passed since, and how much cache re-billing it has `carried` since it entered (`size x turns remaining` -- the number that makes early reduction's leverage visible). Whether that product is exact is read off the session rather than assumed: a transcript carrying a compaction boundary had items leave, so its `carried` is labelled an upper bound and the report names when they left; one carrying none keeps the exact reading. `-- <path>` narrows to one path's loads. Ends with the accounted-vs-unaccounted split: what itok can attribute against what the model actually received, each naming its method. Report-only; per-load sizes are estimates (`bytes/4`).",
    },
    Command {
        name: "headroom",
        synopsis: "headroom [<session>] [--model X] [--window N] [--task N] [-h] [--format human|json] [-C dir]",
        blurb: "`df` for a context: window, used, avail, use% -- plus the growth rate over the last 10/50/200 TURNS (context grows per turn, so a per-second rate would be meaningless) and `~turns left` at the recent rate. Without `--window` or `--model` there is no capacity, so `avail`/`use%`/`turns left` are reported as absent rather than computed against a guessed window. `--task N` adds a `tasks left` column (`avail`/N) -- N is the WINDOW a task occupies, not what it bills. Report-only.",
    },
    Command {
        name: "calibrate",
        synopsis: "calibrate [<session>] [-h] [--format human|json] [-C dir]",
        blurb: "What this session's context actually cost, against what itok estimated: a fixed overhead the transcript cannot see (system prompt + tool schemas) and a scale from `bytes/4` to real tokens. Reports the error BAND measured on turns the fit never saw, plus `n` -- never a bare factor. Too few turns reports `n` and no factor. The scale absorbs message framing and unrecorded reasoning, so it is not a tokenizer ratio and is derived per session. Report-only.",
    },
    Command {
        name: "rate",
        synopsis: "rate [<session>] [--statusline] [--color auto|always|never] [--format human|json] [-C dir]",
        blurb: "Pre-formatted throughput string for a statusline badge: last turn's billed input, the session's gross bill, and GROWTH per hour and per day -- each shortened with ceiling rounding (900 tokens shows as `1k`, never `0k`). Growth is the sum of positive window deltas, which is what predicts a compaction; the bill is 92.5-99.8% cache re-reads, so a rate divided out of it measures how long the session has run rather than how full it is getting, and it climbs fastest when growth is slowest. The bill is still shown, under a name that says what it is -- it is what the API was asked to charge, which is not the same as money (cache reads bill at a fraction, cache creation at a premium) and not the same as content. json keeps `total`, `per_hour` and `per_day` at their old meanings and adds `growth`, `growth_per_hour`, `growth_per_day`, `entered`, `cache_read` and `output`, so the quantities that are easy to confuse can be told apart. Output tokens are reported but not counted anywhere else: itok has never measured that axis. When a red line resolved and the context is growing, a fifth cell gives the time to it at the recent growth rate -- floored, because a deadline rounded up is confidence you have not got, and tilde'd, because it is an extrapolation on an extrapolated threshold. It is in ACTIVE seconds, the same clock the rates divide by, and json says so in the key: `compact_in_active_seconds`. No red line, or a context that is not filling, and there is no estimate and no fifth cell -- infinity is not a number and a permanent dash is not a reading. A value whose period the sample does not cover carries `~`: an hour-rate off twenty minutes is a projection, not a measurement, and the mark says so -- in json it is a `projected_hour`/`projected_day` boolean instead. Rates divide by ACTIVE time, every gap between turns credited up to 300 seconds and no further, rather than by the wall-clock span between the first and last turn: a session left open overnight counts the work, not the sleep. json carries both clocks, `age_seconds` for the span and `active_seconds` for what divides. `--color` reads `[rate]` thresholds from `itok.toml` for per-metric ANSI coloring (green/amber/red independently per value); without a config file the flows are uncolored. The FIRST value is an occupancy level rather than a flow, so it is colored as a fill gauge against the point the session is heading for: `[rate].compact` when declared, else the auto-compact point the harness recorded in this session's own transcript, else the model's window. Where `COLORTERM` says the terminal can show one, the ramp is 24-bit and runs green to red on a sqrt curve, holding green through the flat half; otherwise it degrades to the same three bands cut from the same fraction. The last tenth also takes a `!`, so the warning survives a monochrome statusline and a red-green color blindness. With no capacity and no observation the level falls back to `[rate].turn`, and to plain text without that; a zero window is never painted at all, because an absent measurement is not an empty context. json adds `compact_at` and `compact_n` -- the point, and how many observations stand behind it. `--statusline` reads the harness statusline payload on stdin, taking the transcript and directory from it and emitting the wrapped badge `(itok:...)` -- so the badge reports the session it is drawn beside instead of guessing the newest one in the directory; color defaults to `always` there, since the harness captures the string and no tty is left to detect. 0-1 turns = empty output (badge hidden). Report-only.",
    },
    Command {
        name: "cap",
        synopsis: "cap [N] [--footer human|json]",
        blurb: "Token-budget filter for a pipe: reads stdin, emits the longest whole-line prefix that fits N tokens, and ANNOUNCES the cut in a footer -- what was kept, what was elided, and the line and byte offset to resume from, so the next read continues rather than restarting (the line number is exact on any stream; the byte offset is of the decoded text, so it is for UTF-8 input). `head` truncates silently by lines or bytes; this truncates by tokens and says so. Without N nothing is cut and the footer just reports the cost. Report-only, exit 0.",
    },
    Command {
        name: "docs",
        synopsis: "docs",
        blurb: "Print this command reference as markdown -- the source for README's generated block, kept in sync by a guard.",
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

fn section(c: &Command) -> String {
    format!(
        "### `{}`\n\n```text\n{}\n```\n\n{}\n\n",
        c.name, c.synopsis, c.blurb
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
