//! The exact tier's LAN backend (V22): ollama's own tokenizer over plain
//! HTTP on :11434 -- keyless, free, on-LAN. `count` gets a model's TRUE
//! prompt token count (`prompt_eval_count` from a `num_predict:0`
//! generate), so a local qwen/llama is measured true, not by the o200k
//! proxy that is wrong for it. `models`/`window` discover what a host
//! serves and each model's context window (`/api/tags`, `/api/show`).
//!
//! Behind the `ollama` feature (V23): the core ships zero network deps.
//! Every call is fallible -- a missing host / model / field -- and the
//! caller maps any `Err` to exit 7 (the network code). Non-deterministic +
//! network, so NEVER on `check`/`log` (V5/V19); content leaves the process
//! to the endpoint -- the private LAN path vs cloud (V22).

use std::time::Duration;

pub(crate) mod hosts;
pub(crate) mod pick;

const DEFAULT_PORT: &str = "11434";

/// Normalize one `[scheme://]host[:port]` into a full base URL: default
/// scheme `http` (plain, no TLS -- ollama is not cloud), default port
/// 11434 (V25, the port lives in the host, never a `--ollama-port` flag).
/// Host RESOLUTION (list / stdin / `.context-hosts` / env) is `hosts`.
pub(crate) fn base(host: &str) -> String {
    let (scheme, rest) = host.split_once("://").unwrap_or(("http", host));
    let authority = if rest.contains(':') {
        rest.to_owned()
    } else {
        format!("{rest}:{DEFAULT_PORT}")
    };
    format!("{scheme}://{authority}")
}

/// The EXACT prompt-token count of `text` under `model`: the
/// `prompt_eval_count` a `num_predict:0` generate reports (V22).
pub(crate) fn count(
    base: &str,
    model: &str,
    text: &str,
) -> Result<u64, String> {
    let resp =
        post(&format!("{base}/api/generate"), &generate_body(model, text))?;
    prompt_eval_count(&resp)
        .ok_or_else(|| "no prompt_eval_count in response".to_owned())
}

/// The models a host serves (`/api/tags` -> each `name`), in list order.
pub(crate) fn models(base: &str) -> Result<Vec<String>, String> {
    Ok(model_names(&get(&format!("{base}/api/tags"))?))
}

/// A model's context window (`/api/show` -> `<arch>.context_length`).
pub(crate) fn window(base: &str, model: &str) -> Result<u64, String> {
    let resp = post(&format!("{base}/api/show"), &show_body(model))?;
    context_length(&resp)
        .ok_or_else(|| format!("no context_length for '{model}'"))
}

/// The `/api/generate` body: `num_predict:0` asks for the prompt eval
/// without generation; the count comes back regardless (V22). Pure, so the
/// wire shape is unit-tested without a server.
pub(crate) fn generate_body(model: &str, text: &str) -> String {
    format!(
        "{{\"model\":\"{}\",\"prompt\":\"{}\",\"stream\":false,\
         \"options\":{{\"num_predict\":0}}}}",
        crate::json::escape(model),
        crate::json::escape(text),
    )
}

fn show_body(model: &str) -> String {
    format!("{{\"model\":\"{}\"}}", crate::json::escape(model))
}

/// Pull `prompt_eval_count` out of a generate response.
pub(crate) fn prompt_eval_count(resp: &str) -> Option<u64> {
    number_after(resp, "\"prompt_eval_count\"")
}

/// Pull the context window out of a `/api/show` response. The key is
/// architecture-prefixed (`qwen3moe.context_length`), so match the suffix.
pub(crate) fn context_length(resp: &str) -> Option<u64> {
    number_after(resp, "context_length\"")
}

/// The `name` of each model object in a `/api/tags` response, in order.
pub(crate) fn model_names(resp: &str) -> Vec<String> {
    resp.match_indices("\"name\":\"")
        .filter_map(|(i, key)| {
            let start = i.checked_add(key.len())?;
            let tail = resp.get(start..)?;
            tail.get(..tail.find('"')?).map(str::to_owned)
        })
        .collect()
}

/// The first unsigned integer following `key` in `s`, or None. Tolerant of
/// whitespace/colon between the key and the digits.
fn number_after(s: &str, key: &str) -> Option<u64> {
    let start = s.find(key)?.checked_add(key.len())?;
    let tail = s.get(start..)?;
    let digits: String = tail
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// A blocking agent with a short connect timeout, so an absent host fails
/// fast to exit 7 rather than hanging.
fn agent() -> ureq::Agent {
    ureq::builder()
        .timeout_connect(Duration::from_secs(3))
        .build()
}

fn post(url: &str, body: &str) -> Result<String, String> {
    agent()
        .post(url)
        .set("content-type", "application/json")
        .send_string(body)
        .map_err(|e| format!("ollama POST {url}: {e}"))?
        .into_string()
        .map_err(|e| format!("ollama read {url}: {e}"))
}

fn get(url: &str) -> Result<String, String> {
    agent()
        .get(url)
        .call()
        .map_err(|e| format!("ollama GET {url}: {e}"))?
        .into_string()
        .map_err(|e| format!("ollama read {url}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_adds_scheme_and_default_port() {
        // V25: default scheme http, default port 11434; both kept if given.
        let name = crate::testutil::HOST_NAME;
        let ip = crate::testutil::HOST_IP_ALT;
        assert_eq!(base(name), format!("http://{name}:11434"));
        assert_eq!(base(&format!("{ip}:11434")), format!("http://{ip}:11434"));
        assert_eq!(base("http://box:1234"), "http://box:1234");
        assert_eq!(base("https://proxy"), "https://proxy:11434");
    }

    #[test]
    fn generate_body_carries_model_prompt_and_no_generation() {
        let b = generate_body("qwen3-coder:30b", "hi");
        assert!(b.contains("\"model\":\"qwen3-coder:30b\""));
        assert!(b.contains("\"prompt\":\"hi\""));
        assert!(b.contains("\"num_predict\":0"));
    }

    #[test]
    fn generate_body_escapes_its_prompt() {
        assert!(generate_body("m", "say \"hi\"").contains("say \\\"hi\\\""));
    }

    #[test]
    fn prompt_eval_count_reads_the_number() {
        let r = "{\"response\":\"x\",\"done\":true,\"prompt_eval_count\":10}";
        assert_eq!(prompt_eval_count(r), Some(10));
        assert_eq!(prompt_eval_count("{\"done\":true}"), None);
    }

    #[test]
    fn context_length_reads_the_arch_prefixed_key() {
        // The real /api/show shape: the key carries the architecture.
        let r = "{\"model_info\":{\"qwen3moe.context_length\":262144}}";
        assert_eq!(context_length(r), Some(262_144));
        assert_eq!(context_length("{\"model_info\":{}}"), None);
    }

    #[test]
    fn model_names_lists_each_name_in_order() {
        let r = "{\"models\":[{\"name\":\"a:1\",\"model\":\"a:1\"},\
                 {\"name\":\"b:2\"}]}";
        assert_eq!(model_names(r), vec!["a:1".to_owned(), "b:2".to_owned()]);
        assert!(model_names("{\"models\":[]}").is_empty());
    }
}
