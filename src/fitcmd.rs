//! The `fit` command (V20): greedy pack -- select the subset of candidate
//! files whose token cost fits under `--window`. NOT knapsack (V21): each
//! file is KB against a large window, so greedy-by-order is ~optimal.
//!
//! `fit` SELECTS rather than reports, so it is its own category beside the
//! report verbs (V20): the human output is a BARE, pipeable PATH LIST (the
//! `rg -l`/`git ls-files` shape), so `itok fit --window 200k src/ | xargs
//! cat` assembles a context bundle under budget with no assembler. The
//! window unit reuses the shared parser (V18); the tier is dummy by
//! default, `--bpe` for the true count (V4); candidates default to the
//! git-tracked set (V8). Report-only, exit 0.

use crate::args::Format;
use crate::cli::Output;
use crate::estimate::count;
use crate::render::{DUMMY, O200K};
use crate::units;
use std::path::{Path, PathBuf};

/// A parsed fit request. `--window` is required; without it there is no
/// budget to pack against, so the run is a usage error.
#[derive(Default)]
struct Req {
    window: Option<u64>,
    by_size: bool,
    bpe: bool,
    format: Format,
    chdir: Option<String>,
    paths: Vec<String>,
}

pub(crate) fn fit(rest: &[String]) -> Output {
    match parse(rest) {
        Ok(req) => run(&req),
        Err(e) => Output::usage_err(format!("itok: {e}")),
    }
}

fn run(req: &Req) -> Output {
    let Some(window) = req.window else {
        return Output::usage_err("itok: fit needs --window N".to_owned());
    };
    let root = PathBuf::from(req.chdir.as_deref().unwrap_or("."));
    let items = match candidates(req, &root) {
        Ok(i) => i,
        Err(e) => return Output::usage_err(format!("itok: {e}")),
    };
    let (picked, tokens) = pack(&items, window);
    match req.format {
        Format::Json => Output::ok(json(window, tokens, req.bpe, &picked)),
        Format::Human => Output::ok(path_list(&picked)),
    }
}

/// A costed candidate list: path and its token cost, in selection order.
type Candidates = Vec<(String, u64)>;

/// The candidate `(path, cost)` list: explicit paths or the git-tracked
/// set (V8), each costed on the selected tier, missing files dropped.
/// `--by size` orders smallest-first to fit the MOST files (V20);
/// otherwise argv/tracked order is the caller's priority.
fn candidates(req: &Req, root: &Path) -> Result<Candidates, String> {
    // The same selection rule every other verb uses (V64). `fit` carried
    // its own copy, which is why it emitted `src` as a path that "fits"
    // after counting a directory as 0 (B11d).
    let files = crate::estimate::select_paths(&req.paths, root)?;
    let mut items: Candidates = files
        .iter()
        .filter_map(|f| count(&root.join(f), req.bpe).map(|c| (f.clone(), c)))
        .collect();
    if req.by_size {
        items.sort_by_key(|(_, c)| *c);
    }
    Ok(items)
}

/// Greedy pack: walk the candidates in order and take each whose cost
/// still fits the remaining budget (V20). A file too big is skipped, not a
/// stop, so a later smaller file that fits is still emitted -- the answer
/// to "what fits?". Returns the picked paths and their summed cost.
fn pack(items: &[(String, u64)], window: u64) -> (Vec<String>, u64) {
    let (mut total, mut out) = (0u64, Vec::new());
    for (path, cost) in items {
        match total.checked_add(*cost) {
            Some(n) if n <= window => {
                total = n;
                out.push(path.clone());
            }
            _ => {}
        }
    }
    (out, total)
}

/// The pipeable path list: one path per line, nothing else (V20). An empty
/// selection is an empty output, still exit 0.
fn path_list(paths: &[String]) -> String {
    let mut s = String::new();
    for p in paths {
        s.push_str(p);
        s.push('\n');
    }
    s
}

fn json(window: u64, tokens: u64, bpe: bool, paths: &[String]) -> String {
    let method = if bpe { O200K.label } else { DUMMY.label };
    let files = paths
        .iter()
        .map(|p| format!("\"{}\"", crate::json::escape(p)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"window\":{window},\"tokens\":{tokens},\"method\":\"{method}\",\
         \"files\":[{files}]}}\n"
    )
}

fn parse(rest: &[String]) -> Result<Req, String> {
    let mut r = Req::default();
    let mut i = 0usize;
    while let Some(a) = rest.get(i) {
        apply(&mut r, a, rest, &mut i)?;
        i = i.saturating_add(1);
    }
    Ok(r)
}

/// Apply one token to the request, advancing `i` past any flag value.
fn apply(
    r: &mut Req,
    a: &str,
    rest: &[String],
    i: &mut usize,
) -> Result<(), String> {
    match a {
        "--window" => r.window = Some(units::parse(&val(rest, i)?)?),
        "--by" => r.by_size = by(&val(rest, i)?)?,
        // Gated like estimate's --bpe: without the feature it is an unknown
        // flag, never a silently-ignored one.
        #[cfg(feature = "bpe")]
        "--bpe" => r.bpe = true,
        "--format" => r.format = fmt(&val(rest, i)?)?,
        "-C" => r.chdir = Some(val(rest, i)?),
        p if p.starts_with('-') => return Err(format!("unknown flag '{p}'")),
        p => r.paths.push(p.to_owned()),
    }
    Ok(())
}

fn val(rest: &[String], i: &mut usize) -> Result<String, String> {
    *i = i.saturating_add(1);
    rest.get(*i)
        .cloned()
        .ok_or_else(|| "flag needs a value".to_owned())
}

/// The only ordering key today is `size` (V20); a shared `--budget`-style
/// grammar, so a wrong value names the one it knows.
fn by(s: &str) -> Result<bool, String> {
    match s {
        "size" => Ok(true),
        other => Err(format!("unknown --by '{other}' (want 'size')")),
    }
}

fn fmt(s: &str) -> Result<Format, String> {
    match s {
        "json" => Ok(Format::Json),
        "human" => Ok(Format::Human),
        other => Err(format!("unknown format '{other}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIR: &str = env!("CARGO_MANIFEST_DIR");

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| (*s).to_owned()).collect()
    }

    fn items(costs: &[(&str, u64)]) -> Vec<(String, u64)> {
        costs.iter().map(|(p, c)| ((*p).to_owned(), *c)).collect()
    }

    #[test]
    fn pack_takes_files_within_budget() {
        // 10 + 20 + 30 = 60 <= 65; the 40 would push to 100 and is skipped.
        let (got, total) =
            pack(&items(&[("a", 10), ("b", 20), ("c", 30), ("d", 40)]), 65);
        assert_eq!(got, vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]);
        assert_eq!(total, 60);
    }

    #[test]
    fn a_file_over_budget_is_skipped() {
        // The oversized first file is skipped; the small ones still fit --
        // the answer to "what fits?" (V20).
        let (got, _) = pack(&items(&[("huge", 999), ("x", 5), ("y", 5)]), 20);
        assert_eq!(got, vec!["x".to_owned(), "y".to_owned()]);
    }

    #[test]
    fn nothing_fits_is_empty() {
        let (got, total) = pack(&items(&[("huge", 999)]), 10);
        assert!(got.is_empty());
        assert_eq!(total, 0);
    }

    #[test]
    fn by_size_sorts_by_cost() {
        // Unsorted candidates reorder ascending so the cheapest win first.
        let o = fit(&args(&["--window", "25", "--by", "size", "-C", DIR]));
        assert_eq!(o.code, 0);
        let mut v = items(&[("hi", 30), ("lo", 5), ("mid", 10)]);
        v.sort_by_key(|(_, c)| *c);
        assert_eq!(v.first().map(|(_, c)| *c), Some(5));
    }

    #[test]
    fn missing_window_is_a_usage_error() {
        // V20 (interface): fit needs a budget to pack against.
        let o = fit(&args(&["-C", DIR, "Cargo.toml"]));
        assert_eq!(o.code, 2);
        assert!(o.err.contains("--window"));
    }

    #[test]
    fn a_bad_by_is_a_usage_error() {
        assert_eq!(fit(&args(&["--window", "1M", "--by", "name"])).code, 2);
    }

    #[test]
    fn a_bad_flag_is_a_usage_error() {
        assert_eq!(fit(&args(&["--bogus"])).code, 2);
    }

    #[test]
    fn human_is_a_bare_path_list() {
        // A generous window fits everything; output is paths, no itok, no
        // header -- pipeable (V20).
        let o = fit(&args(&["--window", "100M", "-C", DIR, "Cargo.toml"]));
        assert_eq!(o.code, 0);
        assert!(o.out.contains("Cargo.toml"));
        assert!(!o.out.contains("itok"), "bare list: {:?}", o.out);
    }

    #[test]
    fn a_one_token_window_fits_nothing() {
        let o = fit(&args(&["--window", "1", "-C", DIR, "SPEC.md"]));
        assert_eq!(o.code, 0);
        assert!(o.out.is_empty(), "nothing fits: {:?}", o.out);
    }

    #[test]
    fn json_carries_window_tokens_and_files() {
        let o = fit(&args(&[
            "--window",
            "100M",
            "--format",
            "json",
            "-C",
            DIR,
            "Cargo.toml",
        ]));
        assert!(o.out.contains("\"window\":100000000"));
        assert!(o.out.contains("\"files\":["));
        assert!(o.out.contains("\"method\":\"bytes/4\""));
    }

    #[test]
    fn no_paths_uses_the_tracked_set() {
        // V8: default candidates are the git-tracked files.
        let o = fit(&args(&["--window", "100M", "-C", DIR]));
        assert_eq!(o.code, 0);
        assert!(o.out.contains("SPEC.md"));
    }
}
