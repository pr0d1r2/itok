use std::fmt::Write;
use std::path::Path;

pub fn gate_steps(pkl: &str) -> (usize, usize) {
    let mut section = None;
    let mut fast = 0;
    let mut extra = 0;
    for line in pkl.lines() {
        if line.starts_with("local fast") {
            section = Some(true);
        } else if line.starts_with("local all") {
            section = Some(false);
        } else if line.starts_with('}') {
            section = None;
        } else if line.starts_with("  [\"") && line.ends_with("{") {
            match section {
                Some(true) => fast += 1,
                Some(false) => extra += 1,
                None => {}
            }
        }
    }
    (fast, fast + extra)
}

pub fn direct_dependencies(toml: &str) -> Result<usize, String> {
    let mut section = false;
    let mut count = 0;
    for line in toml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            section = trimmed == "[dependencies]";
            continue;
        }
        if section
            && !trimmed.starts_with('#')
            && !trimmed.is_empty()
            && !line.starts_with(' ')
            && !line.starts_with('\t')
            && trimmed.split_once('=').is_some()
        {
            count += 1;
        }
    }
    if count == 0 {
        return Err("Cargo.toml: [dependencies] is empty".into());
    }
    Ok(count)
}

pub fn nixpkgs_rev(lock: &str) -> Result<String, String> {
    let mut node = false;
    let mut depth = 0usize;
    let mut locked = false;
    for line in lock.lines() {
        if line.trim() == "\"nixpkgs\": {" {
            node = true;
            locked = false;
            depth = 1;
            continue;
        }
        if node && line.trim() == "\"locked\": {" {
            locked = true;
        }
        if node && locked && line.trim_start().starts_with("\"rev\":") {
            let value = line
                .split('"')
                .nth(3)
                .ok_or("flake.lock: malformed nixpkgs rev")?;
            return Ok(value.chars().take(7).collect());
        }
        if node {
            depth = depth.saturating_add(line.matches('{').count());
            depth = depth.saturating_sub(line.matches('}').count());
            if depth == 0 {
                node = false;
                locked = false;
            }
        }
    }
    Err("flake.lock: nixpkgs node/rev is missing".into())
}

pub fn runners(ci: &str) -> Result<Vec<String>, String> {
    let start = ci.find("os: [").ok_or("ci.yml: os matrix is missing")? + 5;
    let end = ci[start..]
        .find(']')
        .ok_or("ci.yml: os matrix is unterminated")?
        + start;
    let values: Vec<_> = ci[start..end]
        .split(',')
        .map(|s| s.trim().trim_matches('[').trim_matches(']').trim())
        .filter(|s| !s.is_empty())
        .collect();
    if values.iter().any(|s| {
        !matches!(
            *s,
            "ubuntu-latest"
                | "ubuntu-24.04-arm"
                | "macos-latest"
                | "macos-15-intel"
        )
    }) {
        let bad = values
            .iter()
            .find(|s| {
                !matches!(
                    **s,
                    "ubuntu-latest"
                        | "ubuntu-24.04-arm"
                        | "macos-latest"
                        | "macos-15-intel"
                )
            })
            .ok_or("ci.yml: unknown runner")?;
        return Err(format!("ci.yml: unknown runner {bad}"));
    }
    Ok(values.into_iter().map(String::from).collect())
}

fn platform_lines(ci: &str) -> Result<String, String> {
    let rs = runners(ci)?;
    let mut out = String::new();
    let mut add = |v: &str, o: &str| {
        let _ = writeln!(
            out,
            "[![{v} {o}](https://img.shields.io/badge/{o}-5277C3?logo={v}&logoColor=white)](flake.nix)"
        );
    };
    let mut platforms = Vec::new();
    for r in rs {
        match r.as_str() {
            "ubuntu-latest" => {
                platforms.extend([("intel", "linux"), ("amd", "linux")]);
            }
            "ubuntu-24.04-arm" => platforms.push(("arm", "linux")),
            "macos-latest" => {
                platforms.extend([("intel", "macos"), ("amd", "macos")]);
            }
            "macos-15-intel" => platforms.push(("arm", "macos")),
            _ => unreachable!(),
        }
    }
    platforms.sort_by_key(|(v, o)| match (*v, *o) {
        ("intel", "linux") => 0,
        ("intel", "macos") => 1,
        ("amd", "linux") => 2,
        ("amd", "macos") => 3,
        ("arm", "linux") => 4,
        ("arm", "macos") => 5,
        _ => 6,
    });
    for (v, o) in platforms {
        add(v, o);
    }
    Ok(out)
}

pub fn splice(readme: &str, block: &str) -> Result<String, String> {
    let lines: Vec<_> = readme.lines().collect();
    let begins: Vec<_> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| **l == "<!-- BEGIN badges -->")
        .map(|(i, _)| i)
        .collect();
    let ends: Vec<_> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| **l == "<!-- END badges -->")
        .map(|(i, _)| i)
        .collect();
    if begins.len() != 1 || ends.len() != 1 || begins[0] >= ends[0] {
        return Err(
            "README.md: badge markers are missing, duplicated, or misordered"
                .into(),
        );
    }
    let mut out = lines[..=begins[0]].join("\n");
    out.push('\n');
    out.push_str(block.trim_end_matches('\n'));
    out.push('\n');
    out.push_str(&lines[ends[0]..].join("\n"));
    out.push('\n');
    Ok(out)
}

pub fn render(
    cargo: &str,
    lock: &str,
    ci: &str,
    hk: &str,
    coverage: &str,
) -> Result<String, String> {
    let edition = cargo
        .lines()
        .find_map(|l| {
            l.strip_prefix("edition = \"")
                .and_then(|x| x.strip_suffix('"'))
        })
        .ok_or("Cargo.toml: edition is missing")?;
    let msrv = cargo
        .lines()
        .find_map(|l| {
            l.strip_prefix("rust-version = \"")
                .and_then(|x| x.strip_suffix('"'))
        })
        .ok_or("Cargo.toml: rust-version is missing")?;
    let deps = direct_dependencies(cargo)?;
    let floor = hk
        .split("fail-under-lines ")
        .skip(1)
        .find_map(|x| {
            x.split_whitespace()
                .next()
                .filter(|v| v.chars().all(|c| c.is_ascii_digit()))
        })
        .ok_or("hk.pkl: coverage floor is missing")?;
    let cov = coverage
        .lines()
        .find_map(|l| {
            l.strip_prefix("lines ")
                .and_then(|x| x.parse::<f64>().ok())
                .map(|x| ((x * 10.0) as u64) as f64 / 10.0)
        })
        .ok_or(".coverage: lines value is missing")?;
    let cov = format!("{cov:.1}");
    let rev = nixpkgs_rev(lock)?;
    let platforms = platform_lines(ci)?;
    let mut out = format!(
        "[![CI](https://github.com/pr0d1r2/itok/actions/workflows/ci.yml/badge.svg)](https://github.com/pr0d1r2/itok/actions/workflows/ci.yml)\n[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)\n[![crates.io](https://img.shields.io/crates/v/itok.svg)](https://crates.io/crates/itok)\n[![docs.rs](https://docs.rs/itok/badge.svg)](https://docs.rs/itok)\n[![edition {edition}](https://img.shields.io/badge/edition-{edition}-000000?logo=rust&logoColor=white)](Cargo.toml)\n[![MSRV {msrv}](https://img.shields.io/badge/MSRV-{msrv}-000000?logo=rust&logoColor=white)](Cargo.toml)\n[![direct dependencies {deps}](https://img.shields.io/badge/direct_dependencies-{deps}-brightgreen)](docs/THIRD-PARTY-NOTICES.md)\n[![minimal tier 0 dependencies](https://img.shields.io/badge/minimal_tier-0_dependencies-brightgreen)](docs/THIRD-PARTY-NOTICES.md)\n[![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-brightgreen)](Cargo.toml)\n[![gate hk](https://img.shields.io/badge/gate-hk-6E4AFF)](hk.pkl)\n[![coverage {cov}%](https://img.shields.io/badge/coverage-{cov}%25-brightgreen)](hk.pkl)\n[![floor {floor}%](https://img.shields.io/badge/floor-%E2%89%A5{floor}%25-brightgreen)](hk.pkl)\n\n[![nix flake](https://img.shields.io/badge/nix-flake-5277C3?logo=nixos&logoColor=white)](flake.nix)\n[![nixpkgs {rev}](https://img.shields.io/badge/nixpkgs-{rev}-5277C3?logo=nixos&logoColor=white)](flake.lock)\n{platforms}\n[![built with Claude Code](https://img.shields.io/badge/built_with-Claude_Code-D97757)](https://claude.com/claude-code)\n[![built with Opus 5](https://img.shields.io/badge/built_with-Opus_5-D97757)](https://www.anthropic.com/claude)\n[![built with SDD](https://img.shields.io/badge/built_with-spec--driven_development-D97757)](SPEC.md)"
    );
    let (fast_steps, all_steps) = gate_steps(hk);
    let badge = format!(
        "[![gate steps {fast_steps} fast / {all_steps} all](https://img.shields.io/badge/gate_steps-{fast_steps}_fast_%2F_{all_steps}_all-6E4AFF)](hk.pkl)\n"
    );
    out = out.replace("[![gate hk](https://img.shields.io/badge/gate-hk-6E4AFF)](hk.pkl)\n", &format!("[![gate hk](https://img.shields.io/badge/gate-hk-6E4AFF)](hk.pkl)\n{badge}"));
    Ok(std::mem::take(&mut out))
}

pub fn run(args: &[String], root: &Path) -> u8 {
    if args.len() < 2 {
        eprintln!(
            "usage: itok-dev readme [--check|--fix] [<path>...] or itok-dev --check [<path>...] / --fix"
        );
        return 2;
    }
    let check = args.iter().any(|a| a == "--check");
    let fix = args.iter().any(|a| a == "--fix");
    if !check && !fix {
        eprintln!("usage: itok-dev readme [--check|--fix]");
        return 2;
    }
    if args.iter().any(|a| a == "integration-doc") {
        let pkl = match std::fs::read_to_string(root.join("hk.pkl")) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("itok-dev: {error}");
                return 1;
            }
        };
        let doc =
            match std::fs::read_to_string(root.join("docs/INTEGRATION.md")) {
                Ok(value) => value,
                Err(error) => {
                    eprintln!("itok-dev: {error}");
                    return 1;
                }
            };
        let (fast, all) = gate_steps(&pkl);
        let mut missing_count = false;
        let labeled: Vec<_> = doc
            .lines()
            .filter(|line| line.contains("steps"))
            .filter_map(|line| {
                let label = match (
                    line.contains("`fast`") || line.contains("— fast"),
                    line.contains("`all`") || line.contains("— all"),
                ) {
                    (true, false) => Some(fast),
                    (false, true) => Some(all),
                    _ => None,
                }?;
                let count = line
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .windows(2)
                    .find_map(|pair| {
                        if !pair[1].contains("step") {
                            return None;
                        }
                        let digits: String = pair[0]
                            .chars()
                            .filter(|c| c.is_ascii_digit())
                            .collect();
                        digits.parse::<usize>().ok()
                    });
                let count = match count {
                    Some(count) => count,
                    None => {
                        missing_count = true;
                        return None;
                    }
                };
                Some((label, count))
            })
            .collect();
        if missing_count
            || !labeled.contains(&(fast, fast))
            || !labeled.contains(&(all, all))
            || labeled.iter().any(|(expected, actual)| expected != actual)
        {
            eprintln!(
                "itok-dev: docs/INTEGRATION.md must state exactly the fast count ({fast}) and all count ({all})"
            );
            return 1;
        }
        return 0;
    }
    let values = (|| {
        let readme = std::fs::read_to_string(root.join("README.md"))
            .map_err(|e| e.to_string())?;
        let cargo = std::fs::read_to_string(root.join("Cargo.toml"))
            .map_err(|e| e.to_string())?;
        let lock = std::fs::read_to_string(root.join("flake.lock"))
            .map_err(|e| e.to_string())?;
        let ci = std::fs::read_to_string(root.join(".github/workflows/ci.yml"))
            .map_err(|e| e.to_string())?;
        let hk = std::fs::read_to_string(root.join("hk.pkl"))
            .map_err(|e| e.to_string())?;
        let coverage = std::fs::read_to_string(root.join(".coverage"))
            .map_err(|e| e.to_string())?;
        let block = render(&cargo, &lock, &ci, &hk, &coverage)?;
        splice(&readme, &block)
    })();
    match values {
        Ok(want) => {
            if check {
                if std::fs::read_to_string(root.join("README.md"))
                    .ok()
                    .as_deref()
                    == Some(&want)
                {
                    0
                } else {
                    eprintln!("itok-dev: README badge block is stale");
                    1
                }
            } else if fix {
                std::fs::write(root.join("README.md"), want).map_or(1, |_| 0)
            } else {
                2
            }
        }
        Err(e) => {
            eprintln!("itok-dev: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wrapped_dependency_and_comments_are_one_each() {
        assert_eq!(
            direct_dependencies(
                "[dependencies]\n# x\nfoo = { version = \"1\", features = [\n \"a\"\n] }\nbar = \"1\"\n"
            ),
            Ok(2)
        );
    }
    #[test]
    fn decoy_lock_node_is_ignored() {
        assert_eq!(
            nixpkgs_rev(
                "\"nixpkgs\": {\n\"inputs\": {\n\"rev\": \"badbad\"\n}\n}\n\"nixpkgs\": {\n\"locked\": {\n\"rev\": \"abcdef0123\"\n}\n}"
            ),
            Ok("abcdef0".into())
        );
    }
    #[test]
    fn markers_are_whole_lines_and_unique() {
        assert!(
            splice("x\n<!-- BEGIN badges -->x\n<!-- END badges -->", "b")
                .is_err()
        );
        assert!(
            splice("<!-- BEGIN badges -->\nold\n<!-- END badges -->", "new")
                .is_ok_and(|s| s.contains("new"))
        );
    }

    #[test]
    fn gate_steps_are_scoped_to_fast_and_all() {
        assert_eq!(
            gate_steps(
                "local fast = new Mapping<String, Step> {\n  [\"check\"] {\n    check = \"hook\"\n  }\n}\nlocal all = (fast) {\n  [\"extra\"] {\n    check = \"step\"\n  }\n}"
            ),
            (1, 2)
        );
    }
}
