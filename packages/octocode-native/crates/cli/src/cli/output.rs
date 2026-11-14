use super::{OutputMode, stdout_is_terminal};
use octocode_native::id::ToolId;
use octocode_native::response::exit::ExitClass;
use octocode_native::response::outcome::ToolOutcome;
use octocode_native::response::wire::wire_json;
use octocode_native::runtime::error::tool_error_result;
use octocode_native::runtime::{FailureKind, RuntimeError};
use serde_json::Value;
#[cfg(unix)]
use serde_json::json;
use std::io::{self, Write};

/// A finished tool call as printed: the warm server sends it whole, so a
/// call answers byte-for-byte alike in process and through the server.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Printed {
    /// The exit code of the call's [`ExitClass`].
    pub(super) exit: u8,
    /// What stdout gets: the serialized JSON (compact) or a view's lines,
    /// without a trailing newline.
    pub(super) body: String,
    /// Whether `body` is the serialized JSON (printed indented on a
    /// terminal) rather than a view's lines.
    pub(super) json: bool,
    /// A failed call's text when its body is the JSON envelope, for a person
    /// watching stderr ([`OutputMode::tells_stderr`]).
    pub(super) error_text: Option<String>,
    /// Text-mode notes for stderr (counts, pages, leads) beside line output.
    pub(super) notes: Option<String>,
}

impl Printed {
    /// The one projection of a finished call, always on stdout: its JSON,
    /// or its view when the result has one and JSON was not asked for; a
    /// rejected call's error envelope as JSON, or as the text of a failed
    /// row. `query` is the call's input: the line format reads whether it
    /// asked for context lines, from the request a cursor continued when
    /// the call was a continuation. `cwd` is the caller's directory (the
    /// client's, under the warm server): a root equal to it needs no note.
    pub(super) fn finish(
        tool: &str,
        result: Result<ToolOutcome, RuntimeError>,
        json_out: bool,
        query: &Value,
        cwd: Option<&std::path::Path>,
    ) -> Self {
        match result {
            Ok(outcome) => {
                let structured = &outcome.structured_content;
                let query = outcome.continued.as_ref().unwrap_or(query);
                // Text prints each row's content, then the facts that decide
                // "am I done" ([`super::lines::render`]).
                let view = ToolId::from_name(tool)
                    .filter(|_| !json_out)
                    .and_then(|id| super::lines::render(id, structured, query, cwd));
                let (body, json, notes) = match view {
                    Some(text) => (
                        text.out,
                        false,
                        Some(text.notes).filter(|notes| !notes.is_empty()),
                    ),
                    None => (wire_json(structured), true, None),
                };
                Self {
                    exit: exit_code(outcome.exit_class()),
                    body,
                    json,
                    error_text: None,
                    notes,
                }
            }
            // A rejected call states its outcome on stdout as a failed row
            // does: stdout alone tells it from an empty answer.
            Err(error) => {
                let projected = tool_error_result(tool, &error);
                let (body, error_text) = if json_out {
                    // The text beside the JSON is for a person's terminal:
                    // a value in it prints as data, never as a control code.
                    (
                        wire_json(&projected.envelope),
                        Some(super::lines::printable_text(&projected.text)),
                    )
                } else {
                    (super::lines::rejected(&projected.envelope), None)
                };
                Self {
                    exit: exit_code(error.exit_class()),
                    body,
                    json: json_out,
                    error_text,
                    notes: None,
                }
            }
        }
    }

    /// The warm server's reply line: `{exit, body, json[, errorText,
    /// notes]}`.
    #[cfg(unix)]
    pub(super) fn to_json(&self) -> Value {
        let mut reply = json!({"exit": self.exit, "body": self.body, "json": self.json});
        if let Some(text) = &self.error_text {
            reply["errorText"] = json!(text);
        }
        if let Some(notes) = &self.notes {
            reply["notes"] = json!(notes);
        }
        reply
    }

    /// A reply line read back; `None` when it is not one.
    #[cfg(unix)]
    pub(super) fn from_json(mut reply: Value) -> Option<Self> {
        Some(Self {
            exit: u8::try_from(reply["exit"].as_u64()?).ok()?,
            body: match reply["body"].take() {
                Value::String(body) => body,
                _ => return None,
            },
            json: reply["json"].as_bool()?,
            error_text: match reply["errorText"].take() {
                Value::String(text) => Some(text),
                Value::Null => None,
                _ => return None,
            },
            notes: match reply["notes"].take() {
                Value::String(text) => Some(text),
                Value::Null => None,
                _ => return None,
            },
        })
    }

    /// Print the body in the call's output `mode`, and a JSON error's text
    /// on stderr when [`OutputMode::tells_stderr`]; a failed write (other
    /// than a closed pipe) is exit 5.
    pub(super) fn print(&self, mode: OutputMode) -> u8 {
        let printed = if self.json {
            write_serialized_json(&self.body)
        } else {
            let printed = if self.body.is_empty() {
                0
            } else {
                write_text(&self.body)
            };
            if let Some(notes) = &self.notes {
                let _ = io::stdout().lock().flush();
                eprintln!("{notes}");
            }
            printed
        };
        if let Some(text) = self.error_text.as_deref()
            && mode.tells_stderr()
        {
            eprintln!("{text}");
        }
        if printed == 0 { self.exit } else { printed }
    }
}

pub(super) fn exit_code(class: ExitClass) -> u8 {
    match class {
        ExitClass::Success => 0,
        ExitClass::Empty => 1,
        ExitClass::InvalidInput => 2,
        ExitClass::Failed(FailureKind::NotFound) => 3,
        ExitClass::Failed(FailureKind::Authentication | FailureKind::Permission) => 4,
        ExitClass::Failed(FailureKind::Execution) => 5,
        ExitClass::Failed(FailureKind::RateLimited) => 7,
    }
}

pub(in super::super) fn write_text(text: &str) -> u8 {
    let text = text.strip_suffix('\n').unwrap_or(text);
    match writeln!(io::stdout().lock(), "{text}") {
        Ok(()) => 0,
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => 0,
        Err(_) => 5,
    }
}

/// Print JSON on stdout: the one writer of every JSON answer, indented for a
/// terminal and one compact line on a pipe.
pub(in super::super) fn write_json(value: &Value) -> u8 {
    match json_text(value, stdout_is_terminal()) {
        Ok(text) => write_text(&text),
        Err(_) => 5,
    }
}

/// Print JSON already serialized compactly (a tool call's body, which the
/// warm server sends as text) as [`write_json`] does: unchanged on a pipe.
pub(super) fn write_serialized_json(compact: &str) -> u8 {
    if !stdout_is_terminal() {
        return write_text(compact);
    }
    match serde_json::from_str::<Value>(compact) {
        Ok(value) => write_json(&value),
        Err(_) => write_text(compact),
    }
}

/// `value` as printed on a `terminal` (indented) or a pipe (one line).
pub(super) fn json_text(value: &Value, terminal: bool) -> serde_json::Result<String> {
    if terminal {
        serde_json::to_string_pretty(value)
    } else {
        serde_json::to_string(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octocode_native::runtime::error::mcp_result;
    use octocode_native::security::Sanitized;
    use serde_json::json;

    #[test]
    fn a_paged_fixture_is_byte_identical_on_cli_and_mcp() {
        let fixture = json!({"results":[{"index":0,"data":{"files":[]}}],
            "responsePagination":{"hasMore":true,
                "next":{"kind":"continue","tool":"localSearch","input":{"cursor":"stable"}},
                "expiresAt":"2026-11-02T00:00:00.000Z","replayed":false}});
        let outcome = || ToolOutcome {
            structured_content: Sanitized::scrubbed(fixture.clone()),
            failure: Some(FailureKind::Execution),
            all_failed: false,
            continued: None,
        };
        let cli = Printed::finish("localSearch", Ok(outcome()), true, &Value::Null, None);
        let mcp = mcp_result("localSearch", Ok(outcome())).expect("MCP result");
        assert_eq!(cli.body.as_bytes(), mcp.text.as_bytes());
    }
}
