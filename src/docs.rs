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
        synopsis: "estimate [-s] [-h] [--top N] [--budget N] [--bpe] [--ollama [HOSTS]] [--format human|json] [-C dir] [paths...]",
        blurb: "Token cost of files, git-tracked by default. `--bpe` swaps bytes/4 for a real tokenizer (o200k); `--ollama` gets an exact count from a local model's own tokenizer. `--budget N` turns it into a gate.",
    },
    Command {
        name: "doctor",
        synopsis: "doctor [--model X] [--window N] [--ollama [HOSTS]] [--format human|json] [-C dir] [paths...]",
        blurb: "Advisory health check: fit-to-window, budget balance, noise ratio, estimate confidence. Reports and suggests; never gates. `--model X` resolves an encoding via `.context-models`; `--ollama` discovers live model windows across a fleet.",
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
        blurb: "Ranked context occupancy for a session, `du`-shaped: how much each thing cost, how many times it was loaded, and how many turns have passed since. `-- <path>` narrows to one path's loads. Ends with the accounted-vs-unaccounted split: what itok can attribute against what the model actually received, each naming its method. Report-only; per-load sizes are estimates (`bytes/4`).",
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
         usage: itok <command> [args]\n\ncommands:\n{body}"
    )
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
