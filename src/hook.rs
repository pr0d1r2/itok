//! The harness mapping, and the ONLY module that knows one (V52).
//!
//! `guard` is an adapter: a harness hands it hook JSON on stdin and reads a
//! decision on stdout. Everything about THAT shape lives here, so a second
//! agent harness is a second mapping rather than a fork of the tool
//! (V43's pluggability rule, applied to the enforcement axis).
//!
//! Claude Code's `PreToolUse` contract, which this implements:
//!
//! ```text
//! stdin   {"session_id","transcript_path","cwd","permission_mode",
//!          "hook_event_name":"PreToolUse","tool_name","tool_input":{...}}
//! stdout  {"hookSpecificOutput":{"permissionDecision":"allow|deny|ask"},
//!          "systemMessage":"..."}
//! ```
//!
//! And its statusline contract, which `rate --statusline` implements: the
//! payload names the transcript outright, which is the point -- a badge
//! must report the session it is drawn beside, and inference cannot know
//! which of a directory's transcripts that is (V96).
//!
//! The decision travels in the JSON, never in the exit code (V52 and the
//! section I line for `guard`): the harness reads stdout, and a non-zero exit
//! means the adapter itself failed, which is a different claim entirely.
//!
//! Parsed by hand, like every other reader here: `serde_json` is optional
//! behind a feature and the core carries no required deps. Only four
//! fields are read and unknown ones are ignored, so a harness adding a
//! field never breaks the adapter -- but a harness RENAMING one does, and
//! that is correct: a rename means the mapping is out of date, and
//! guessing past it would decide against a request nobody described.

/// What the guard needs to know about one tool call, harness-agnostic.
///
/// Everything downstream takes THIS, never the hook's JSON, which is what
/// keeps the rest of the tool free of any harness (V52).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Request {
    /// The harness's name for the tool: `Read`, `Bash`, `Edit`.
    pub(crate) tool: String,
    /// The file the call concerns, when it names one. A `Bash` call
    /// usually does not, and inventing a path for it would be a confident
    /// lie (V3).
    pub(crate) path: Option<String>,
    /// Where the harness says the session is rooted -- the directory whose
    /// `.context-policy` applies.
    pub(crate) cwd: Option<String>,
}

/// What the guard decided.
///
/// The harness contract also has `ask`, and it is deliberately ABSENT: no
/// rule this build honors can produce it, and a variant nothing can reach
/// is the same false assurance as an unhonored row (V105). T44's `warn`
/// tier is what will earn it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Decision {
    Allow,
    Deny,
}

impl Decision {
    fn label(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }
}

/// Read one hook payload. A field the mapping does not know is IGNORED; a
/// field it needs and cannot find is `None`, never a guess.
#[must_use]
pub(crate) fn request(stdin: &str) -> Request {
    Request {
        tool: string_field(stdin, "tool_name").unwrap_or_default(),
        // `file_path` lives inside `tool_input`, and it is the only place
        // that key appears in the payload, so a flat search finds it
        // without a nested parse. Stated rather than assumed: if a harness
        // ever puts `file_path` at the top level too, this reads the first.
        path: string_field(stdin, "file_path"),
        cwd: string_field(stdin, "cwd"),
    }
}

/// Where a statusline payload says this statusline's session lives.
///
/// A DIFFERENT harness event from `PreToolUse`, and it lands here for the
/// same reason that one does: this is the only module allowed to know a
/// harness (V52). A shell wrapper pulling `transcript_path` out with `jq`
/// would be a second mapping -- outside the crate, covered by no test,
/// and silently wrong the day the harness renames a field.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct StatusLine {
    /// The transcript THIS statusline belongs to. The whole reason the
    /// payload is read at all: inference is newest-by-mtime for a cwd
    /// (V96), so two concurrent sessions in one repo would each show the
    /// other's numbers.
    pub(crate) transcript: Option<String>,
    /// The session's directory, which is whose `itok.toml` applies.
    pub(crate) cwd: Option<String>,
}

/// Read one statusline payload. Same contract as `request`: an unknown
/// field is ignored, a needed one that is absent is `None`, never a guess.
///
/// ```text
/// stdin  {"session_id","transcript_path","cwd","model":{...},...}
/// ```
#[must_use]
pub(crate) fn statusline(stdin: &str) -> StatusLine {
    StatusLine {
        transcript: string_field(stdin, "transcript_path"),
        cwd: string_field(stdin, "cwd"),
    }
}

/// One string field's value, or `None` when absent or not a string.
fn string_field(text: &str, key: &str) -> Option<String> {
    let at = text.find(&format!("\"{key}\":"))?;
    let tail = text.get(at.checked_add(key.len().checked_add(3)?)?..)?;
    let inner = tail.trim_start().strip_prefix('"')?;
    let end = inner.find('"')?;
    inner.get(..end).map(str::to_owned)
}

/// The decision, in the shape the harness reads.
///
/// `systemMessage` is omitted when there is nothing to say, because output
/// that always appears is output nobody reads (V71). An `allow` with no
/// reason is the silent common case.
#[must_use]
pub(crate) fn response(d: Decision, why: &str) -> String {
    let msg = if why.is_empty() {
        String::new()
    } else {
        format!(",\"systemMessage\":\"{}\"", crate::json::escape(why))
    };
    format!(
        "{{\"hookSpecificOutput\":{{\"permissionDecision\":\"{}\"}}{msg}}}\n",
        d.label()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAYLOAD: &str = r#"{"session_id":"s1","transcript_path":"/t.jsonl",
        "cwd":"/repo","permission_mode":"ask","hook_event_name":"PreToolUse",
        "tool_name":"Read","tool_input":{"file_path":"/repo/big.rs"}}"#;

    /// The statusline payload, in the shape the harness sends it: the
    /// two fields the badge needs, out of a body carrying much more.
    #[test]
    fn a_statusline_payload_names_its_transcript() {
        let text = r#"{"hook_event_name":"Status","session_id":"s1",
            "transcript_path":"/p/s1.jsonl","cwd":"/repo",
            "model":{"id":"claude","display_name":"Opus"},
            "workspace":{"current_dir":"/repo"}}"#;
        assert_eq!(
            statusline(text),
            StatusLine {
                transcript: Some("/p/s1.jsonl".to_owned()),
                cwd: Some("/repo".to_owned()),
            }
        );
    }

    /// A field the mapping needs and cannot find is `None`, never a
    /// guess -- the caller turns that into a usage error rather than
    /// silently reporting some other session.
    #[test]
    fn a_statusline_payload_missing_the_transcript_is_none() {
        let got = statusline(r#"{"session_id":"s1","cwd":"/repo"}"#);
        assert_eq!(got.transcript, None);
        assert_eq!(got.cwd, Some("/repo".to_owned()));
    }

    /// The four fields the guard needs, out of a real payload shape.
    #[test]
    fn a_pretooluse_payload_maps_to_a_request() {
        assert_eq!(
            request(PAYLOAD),
            Request {
                tool: "Read".to_owned(),
                path: Some("/repo/big.rs".to_owned()),
                cwd: Some("/repo".to_owned()),
            }
        );
    }

    /// A call that names no file yields `None`, never a made-up path (V3).
    /// `Bash` is the common case and the one where a guess would be worst.
    #[test]
    fn a_call_without_a_file_has_no_path() {
        let bash = r#"{"tool_name":"Bash","cwd":"/repo",
            "tool_input":{"command":"ls -la"}}"#;
        let got = request(bash);
        assert_eq!(got.tool, "Bash");
        assert_eq!(got.path, None, "no file named, so none reported");
    }

    /// Unknown fields are ignored, so a harness adding one does not break
    /// the adapter.
    #[test]
    fn an_unknown_field_is_ignored() {
        let extra = r#"{"tool_name":"Read","brand_new_field":"whatever",
            "tool_input":{"file_path":"a.rs"}}"#;
        assert_eq!(request(extra).path, Some("a.rs".to_owned()));
    }

    /// V52: the decision is in the JSON. Every variant renders, and the
    /// harness never has to read an exit code to learn the answer.
    #[test]
    fn every_decision_renders_in_the_json() {
        for (d, word) in [(Decision::Allow, "allow"), (Decision::Deny, "deny")]
        {
            let out = response(d, "");
            assert!(
                out.contains(&format!("\"permissionDecision\":\"{word}\""))
            );
        }
    }

    /// V71: silence when there is nothing to say, and a reason when there
    /// is -- escaped, because a path with a quote must not emit JSON no
    /// parser accepts.
    #[test]
    fn a_reason_appears_only_when_there_is_one() {
        assert!(!response(Decision::Allow, "").contains("systemMessage"));
        let out = response(Decision::Deny, "over \"budget\"");
        assert!(out.contains("systemMessage"), "{out}");
        assert!(out.contains(r#"over \"budget\""#), "escaped: {out}");
    }

    /// One line, so a harness reading a line at a time gets one object.
    #[test]
    fn the_response_is_one_line() {
        let out = response(Decision::Deny, "a reason");
        assert_eq!(out.lines().count(), 1, "{out}");
        assert!(out.ends_with('\n'));
    }
}
