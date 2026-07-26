//! Process-boundary smoke only. Everything else -- every verb, flag,
//! error path, and the permutation properties -- is unit- and
//! property-tested against `itok::cli::run` in cli.rs, at lib cost. These
//! three prove the one thing a lib test cannot: that `main` wires
//! `run`'s Output to the actual streams and the real process exit code.

#![allow(clippy::unwrap_used, clippy::panic)]

use assert_cmd::Command;
use predicates::prelude::*;

fn itok() -> Command {
    Command::cargo_bin("itok").unwrap()
}

#[test]
fn stdout_and_exit_zero_on_success() {
    itok()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("itok"));
}

#[test]
fn stderr_and_exit_two_on_usage_error() {
    itok()
        .arg("bogus")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("unknown"));
}

#[test]
fn estimate_reports_to_stdout() {
    itok()
        .args(["estimate", "SPEC.md"])
        .assert()
        .success()
        .stdout(predicate::str::contains("itok"));
}

// V11 at the process boundary: `--model` on doctor. An unknown model is a
// hard failure (exit 2), and a listed model resolves its encoding and
// labels the report -- proving `main` wires the resolver and its error.
#[test]
fn doctor_unknown_model_exits_two() {
    itok()
        .args(["doctor", "--model", "gpt-4o", "SPEC.md"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("unknown model"));
}

#[cfg(feature = "bpe")]
#[test]
fn doctor_honors_a_listed_model() {
    let dir =
        std::env::temp_dir().join(format!("itok-t7-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(".context-models"), "gpt-4o o200k\n").unwrap();
    std::fs::write(dir.join("f.txt"), "hello world\n").unwrap();
    itok()
        .args(["doctor", "--model", "gpt-4o", "f.txt"])
        .current_dir(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("(o200k)"));
}

// V18: a `--model` with a window column gives doctor a fit-line with no
// `--window` flag; an explicit `--window` overrides the table's window.
#[cfg(feature = "bpe")]
#[test]
fn doctor_model_window_gives_the_fit_line() {
    let dir = std::env::temp_dir()
        .join(format!("itok-t14-fit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(".context-models"), "qwen o200k 256k\n").unwrap();
    std::fs::write(dir.join("f.txt"), "hello world\n").unwrap();
    itok()
        .args(["doctor", "--model", "qwen", "f.txt"])
        .current_dir(&dir)
        .assert()
        .success()
        // The fit-line prints the resolved window, not the "pass --window"
        // prompt, because the model row supplied it.
        .stdout(
            predicate::str::contains("256000")
                .and(predicate::str::contains("pass --window").not()),
        );
}

#[cfg(feature = "bpe")]
#[test]
fn explicit_window_overrides_the_model_window() {
    let dir = std::env::temp_dir()
        .join(format!("itok-t14-override-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(".context-models"), "qwen o200k 256k\n").unwrap();
    std::fs::write(dir.join("f.txt"), "hello world\n").unwrap();
    itok()
        .args(["doctor", "--model", "qwen", "--window", "1M", "f.txt"])
        .current_dir(&dir)
        .assert()
        .success()
        // Explicit --window wins: 1M, not the table's 256k (V18).
        .stdout(
            predicate::str::contains("1000000")
                .and(predicate::str::contains("256000").not()),
        );
}

// V20: `fit` selects the subset that fits and emits a bare, pipeable path
// list; --window is required, so its absence is a usage error.
#[test]
fn fit_emits_a_pipeable_path_list() {
    itok()
        .args(["fit", "--window", "100M", "Cargo.toml"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Cargo.toml")
                .and(predicate::str::contains("itok").not()),
        );
}

#[test]
fn fit_without_a_window_is_a_usage_error() {
    itok()
        .args(["fit", "Cargo.toml"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("--window"));
}

// V38: the ollama network backend is CASSETTE-REPLAYED here -- a local
// stub serves the recorded responses (tests/fixtures/ollama.json, standard
// vcr-cassette format) so the full path (cli -> estcmd/discover -> ureq ->
// parse -> render) is exercised offline and deterministically, no server.
// Gated on the feature so it runs in the `--features ollama` CI axis.
#[cfg(feature = "ollama")]
mod ollama_cassette {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use vcr_cassette::Cassette;

    const CASSETTE: &str = include_str!("fixtures/ollama.json");
    const DIR: &str = env!("CARGO_MANIFEST_DIR");

    /// Spawn a replay server that serves each recorded response keyed by
    /// request path, over a random localhost port. Returns `host:port` for
    /// `OLLAMA_HOST`. The thread serves until the test process ends.
    fn replay() -> String {
        let cassette: Cassette = serde_json::from_str(CASSETTE).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let host = listener.local_addr().unwrap().to_string();
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                serve(stream, &cassette);
            }
        });
        host
    }

    fn serve(mut stream: TcpStream, cassette: &Cassette) {
        let req = read_request(&stream);
        let body = response_for(cassette, &request_path(&req));
        let out = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\
             Content-Type: application/json\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(out.as_bytes());
        let _ = stream.flush();
    }

    /// The whole request -- headers AND body -- before we reply.
    ///
    /// TCP is a STREAM: one `read` returns what has ARRIVED, not a request.
    /// The single POST (`/api/generate`) can land its body in a second
    /// segment, and replying after one read drops the socket mid-write, so
    /// the client sees `Invalid argument (os error 22)`. Serial runs happen
    /// to fit one segment; parallel ones do not (B5). Drain to
    /// `Content-Length`, then answer.
    fn read_request(stream: &TcpStream) -> String {
        let mut peek = stream.try_clone().unwrap();
        let mut buf = Vec::new();
        let mut chunk = [0u8; 1024];
        while !is_complete(&buf) {
            let n = peek.read(&mut chunk).unwrap_or(0);
            if n == 0 {
                break;
            }
            buf.extend_from_slice(chunk.get(..n).unwrap_or(&[]));
        }
        String::from_utf8_lossy(&buf).into_owned()
    }

    /// True once the buffer holds the header block plus a full body.
    fn is_complete(buf: &[u8]) -> bool {
        let text = String::from_utf8_lossy(buf);
        let Some(end) = text.find("\r\n\r\n").map(|i| i.saturating_add(4))
        else {
            return false;
        };
        text.len().saturating_sub(end) >= content_length(&text)
    }

    /// `Content-Length` from the header block; 0 when absent (a GET).
    fn content_length(head: &str) -> usize {
        head.lines()
            .filter(|l| l.to_ascii_lowercase().starts_with("content-length:"))
            .find_map(|l| l.split(':').nth(1))
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0)
    }

    /// The path from the request line (`GET /api/tags HTTP/1.1`).
    fn request_path(req: &str) -> String {
        req.split_whitespace().nth(1).unwrap_or("").to_owned()
    }

    /// The recorded response body whose request path matches, else empty.
    fn response_for(cassette: &Cassette, path: &str) -> String {
        cassette
            .http_interactions
            .iter()
            .find(|i| i.request.uri.path() == path)
            .map(|i| i.response.body.string.clone())
            .unwrap_or_default()
    }

    #[test]
    fn estimate_ollama_replays_the_exact_count() {
        // The cassette's /api/generate returns prompt_eval_count=42, tagged
        // the exact tier (V22) -- deterministic, offline.
        let host = replay();
        itok()
            .args(["estimate", "--ollama", "Cargo.toml"])
            .env("OLLAMA_HOST", &host)
            .current_dir(DIR)
            .assert()
            .success()
            // T79: the label NAMES the tokenizer that produced the count,
            // so a number from an unintended endpoint is visible (V101).
            // `(exact)` alone used to render identically for every model
            // on every host.
            .stdout(
                predicate::str::contains("42 itok").and(
                    predicate::str::contains("exact via qwen3-coder:30b@"),
                ),
            );
    }

    #[test]
    fn doctor_ollama_fleet_unions_the_models() {
        // V24: two hosts, each serving qwen3-coder:30b, union to one model.
        let fleet = format!("{},{}", replay(), replay());
        itok()
            .arg("doctor")
            .arg("--ollama")
            .arg(&fleet)
            .current_dir(DIR)
            .assert()
            .success()
            .stdout(
                predicate::str::contains("qwen3-coder:30b")
                    .and(predicate::str::contains("262144")),
            );
    }

    #[test]
    fn doctor_ollama_replays_the_enumerate() {
        // /api/tags -> qwen3-coder:30b, /api/show -> window 262144 (V22).
        let host = replay();
        itok()
            .args(["doctor", "--ollama"])
            .env("OLLAMA_HOST", &host)
            .current_dir(DIR)
            .assert()
            .success()
            .stdout(
                predicate::str::contains("qwen3-coder:30b")
                    .and(predicate::str::contains("262144")),
            );
    }

    /// V9: json keeps `method` as the bare tier a parser matches on, and
    /// carries the endpoint as its OWN field -- folding it into `method`
    /// would break every consumer testing for `"exact"`.
    #[test]
    fn estimate_ollama_json_keeps_method_and_adds_endpoint() {
        let host = replay();
        itok()
            .args(["estimate", "--ollama", "--format", "json", "Cargo.toml"])
            .env("OLLAMA_HOST", &host)
            .current_dir(DIR)
            .assert()
            .success()
            .stdout(predicate::str::contains("\"method\":\"exact\"").and(
                predicate::str::contains("\"endpoint\":\"qwen3-coder:30b@"),
            ));
    }

    /// V6 on a discovered set: a family prefix narrows to the one model
    /// the fleet serves. The cassette serves only `qwen3-coder:30b`, so
    /// `qwen3` is unambiguous and must resolve rather than 404.
    #[test]
    fn doctor_ollama_narrows_by_a_model_prefix() {
        let host = replay();
        itok()
            .args(["doctor", "--ollama", "--model", "qwen3"])
            .env("OLLAMA_HOST", &host)
            .current_dir(DIR)
            .assert()
            .success()
            .stdout(predicate::str::contains("qwen3-coder:30b"));
    }

    /// V71: an unserved name is a USAGE error that NAMES what is served.
    /// The fleet answered, so exit 7 (network) would be a lie -- and a CI
    /// retry loop would spin on what is really a typo.
    #[test]
    fn an_unserved_model_names_the_alternatives() {
        let host = replay();
        itok()
            .args(["doctor", "--ollama", "--model", "no-such-model"])
            .env("OLLAMA_HOST", &host)
            .current_dir(DIR)
            .assert()
            .code(2)
            .stderr(
                predicate::str::contains("no fleet host serves")
                    .and(predicate::str::contains("qwen3-coder:30b")),
            );
    }

    // The LIVE check (V38): never in the gate. Run by hand against a real
    // host: `OLLAMA_HOST=box:11434 cargo test -p itok --features ollama -- \
    // --ignored`.
    #[test]
    #[ignore = "hits a real ollama host; run with --ignored + OLLAMA_HOST"]
    fn live_ollama_smoke() {
        itok()
            .args(["estimate", "--ollama", "README.md"])
            .current_dir(DIR)
            .assert()
            .success()
            .stdout(predicate::str::contains("(exact)"));
    }
}
