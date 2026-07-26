//! Run the host's `cavekit-spec` format checker over a spec file.
//!
//! Prints one line per violation and exits 1 if any. Exit 2 means the file
//! could not be read -- distinct from "clean", because a missing file that
//! reported success would be the silent pass V71 forbids.
fn main() -> std::process::ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: specfmt <SPEC.md>");
        return std::process::ExitCode::from(2);
    };
    let Ok(src) = std::fs::read_to_string(&path) else {
        eprintln!("specfmt: cannot read {path}");
        return std::process::ExitCode::from(2);
    };
    let found = cavekit_spec::check(&src);
    for v in &found {
        println!("{}: {}", v.rule, v.msg);
    }
    println!("{}: {} violation(s)", path, found.len());
    if found.is_empty() {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
