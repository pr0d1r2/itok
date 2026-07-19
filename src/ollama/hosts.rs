//! ollama host resolution (V24/V25). A `--ollama` value is a comma-list of
//! `[scheme://]host[:port]`, or `-` to read the list from stdin; absent, it
//! falls back to `.context-hosts`, then `OLLAMA_HOST`, then localhost. The
//! FLEET is the union of these hosts -- `doctor --ollama box1,box2` asks
//! "which model, anywhere, holds this?" (V24). itok never takes a CIDR: a
//! subnet probe is a port scan, scope-alien and hostile; discovery, if
//! wanted, is COMPOSED (`nmap ... | itok doctor --ollama -`), never in-tool.

use super::base;
use std::io::Read;
use std::path::Path;

const HOSTS_FILE: &str = ".context-hosts";
const DEFAULT: &str = "localhost:11434";

/// The fleet of base URLs to query, in order. Explicit `--ollama VALUE`
/// wins; then `.context-hosts`, then `OLLAMA_HOST`, then localhost. Each
/// host is normalized (scheme + default port, V25); a CIDR is rejected.
pub(crate) fn bases(
    arg: Option<&str>,
    root: &Path,
) -> Result<Vec<String>, String> {
    raw_hosts(arg, root).iter().map(|h| host_base(h)).collect()
}

/// The raw host strings before normalization, by the precedence above.
fn raw_hosts(arg: Option<&str>, root: &Path) -> Vec<String> {
    match arg {
        Some("-") => split(&stdin()),
        Some(list) => split(list),
        None => from_file(root)
            .or_else(from_env)
            .unwrap_or_else(|| vec![DEFAULT.to_owned()]),
    }
}

/// Normalize one host to a base URL, rejecting a CIDR (V24).
fn host_base(host: &str) -> Result<String, String> {
    if is_cidr(host) {
        return Err(format!(
            "no CIDR '{host}' -- list hosts explicitly, itok never scans (V24)"
        ));
    }
    Ok(base(host))
}

/// A `/` in the authority marks a CIDR/subnet (`10.0.0.0/24`), which itok
/// refuses; a `://` scheme separator does not count.
fn is_cidr(host: &str) -> bool {
    host.rsplit("://").next().unwrap_or(host).contains('/')
}

/// Split a comma/whitespace list into non-empty, trimmed hosts.
fn split(list: &str) -> Vec<String> {
    list.split([',', '\n', ' ', '\t'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Hosts from `.context-hosts` (one per line, `#` comments), if the file
/// exists and names any.
fn from_file(root: &Path) -> Option<Vec<String>> {
    let text = std::fs::read_to_string(root.join(HOSTS_FILE)).ok()?;
    let hosts: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect();
    (!hosts.is_empty()).then_some(hosts)
}

/// The single host in `OLLAMA_HOST`, if set (Ollama's own convention, V22).
fn from_env() -> Option<Vec<String>> {
    std::env::var("OLLAMA_HOST")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|h| vec![h])
}

/// The host list piped on stdin (`--ollama -`): the composed-discovery path
/// (`nmap -p11434 ... | itok doctor --ollama -`, V24).
fn stdin() -> String {
    let mut s = String::new();
    let _ = std::io::stdin().read_to_string(&mut s);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_comma_list_becomes_normalized_bases() {
        let root = Path::new("/no/such/dir");
        let got = bases(Some("box1,box2:6666"), root).unwrap_or_default();
        assert_eq!(got, vec!["http://box1:11434", "http://box2:6666"]);
    }

    #[test]
    fn whitespace_and_empties_are_dropped() {
        assert_eq!(split(" a , , b\n"), vec!["a".to_owned(), "b".to_owned()]);
    }

    #[test]
    fn a_cidr_is_rejected() {
        // V24: itok never scans a subnet.
        let e = bases(Some("10.0.0.0/24"), Path::new("."))
            .err()
            .unwrap_or_default();
        assert!(e.contains("CIDR"), "{e}");
        assert!(!is_cidr("http://box:11434"), "scheme :// is not a CIDR");
        assert!(is_cidr("10.0.0.0/24"));
    }

    #[test]
    fn no_arg_no_file_no_env_is_localhost() {
        // The from_env branch reads a process-global; this dir has no
        // .context-hosts, so absent OLLAMA_HOST it is localhost.
        let root = Path::new("/no/such/itok/dir");
        let got = bases(None, root).unwrap_or_default();
        // Either localhost (no env) or the env host -- both are one base.
        assert_eq!(got.len(), 1);
        assert!(got.first().is_some_and(|h| h.starts_with("http")));
    }

    #[test]
    fn context_hosts_file_supplies_the_fleet() {
        let dir = std::env::temp_dir()
            .join(format!("itok-t18-hosts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).ok();
        std::fs::write(dir.join(HOSTS_FILE), "# fleet\nbox1\nbox2:6666\n").ok();
        // With no --ollama value, the file supplies the hosts (unless
        // OLLAMA_HOST is set in the env, which takes lower precedence here).
        let got = from_file(&dir).unwrap_or_default();
        assert_eq!(got, vec!["box1".to_owned(), "box2:6666".to_owned()]);
    }
}
