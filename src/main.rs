//! itok binary: a thin shell over `itok::cli::run`. All logic and its
//! tests live in the lib (`cli.rs`) so the whole command surface is unit-
//! and property-testable at lib cost -- no process spawn (V245 totality
//! is a proptest, not a hope). `main` does only the I/O the core
//! deliberately avoids: print the streams, set the exit code.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let o = itok::cli::run(&args);
    print!("{}", o.out);
    eprint!("{}", o.err);
    ExitCode::from(o.code)
}
