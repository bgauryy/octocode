use super::{OutputMode, stdout_is_terminal};
use octocode_native::id::ToolId;
use octocode_native::response::exit::ExitClass;
use octocode_native::response::outcome::ToolOutcome;
use octocode_native::runtime::error::tool_error_result;
use octocode_native::runtime::{FailureKind, RuntimeError};
use serde_json::Value;
#[cfg(unix)]
use serde_json::json;
use std::io::{self, Write};

/// Where a finished call's output goes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Stream {
    Stdout,
    Stderr,
}

/// A finished tool call as printed: the warm server sends it whole, so a
/// call answers byte-for-byte alike in process and through the server.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Printed {
    /// The exit code of the call's [`ExitClass`].
    pub(super) exit: u8,
    pub(super) stream: Stream,
    /// The serialized JSON (compact) or a view's lines, without a trailing
    /// newline.
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
    /// The one projection of a finished call: its JSON, or its view when
    /// the result has one and JSON was not asked for; or the call's error
    /// envelope (JSON on stdout) or text (stderr).
    /// `query` is the call's input: the line format reads whether it asked
    /// for context lines. `cwd` is the caller's directory (the client's,
    /// under the warm server): a root equal to it needs no note.
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
                // A view prints a single-row search, listing or symbol
                // result in its standard line format; every other result
                // prints the same JSON as `--json`.
                let view = ToolId::from_name(tool)
                    .filter(|_| !json_out)
                    .and_then(|id| super::lines::render(id, structured, query, cwd));
                let (body, json, notes) = match view {
                    Some(text) => (
                        text.out,
                        false,
                        Some(text.notes).filter(|notes| !notes.is_empty()),
                    ),
                    None => (structured.to_string(), true, None),
                };
                // A call whose every row failed prints on stderr, so a pipe
                // never reads an error as data.
                let stream = if !json_out && super::lines::all_rows_failed(structured) {
                    Stream::Stderr
                } else {
                    Stream::Stdout
                };
                Self {
                    exit: exit_code(outcome.exit_class()),
                    stream,
                    body,
                    json,
                    error_text: None,
                    notes,
                }
            }
            Err(error) => {
                let projected = tool_error_result(tool, &error);
                let (stream, body, error_text, notes) = if json_out {
                    let envelope = projected.envelope.to_string();
                    (Stream::Stdout, envelope, Some(projected.text), None)
                } else {
                    (Stream::Stderr, projected.text, None, None)
                };
                Self {
                    exit: exit_code(error.exit_class()),
                    stream,
                    body,
                    json: json_out,
                    error_text,
                    notes,
                }
            }
        }
    }

    /// The warm server's reply line: `{exit, stderr, body, json[, errorText,
    /// notes]}`.
    #[cfg(unix)]
    pub(super) fn to_json(&self) -> Value {
        let mut reply = json!({"exit": self.exit, "stderr": self.stream == Stream::Stderr,
            "body": self.body, "json": self.json});
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
            stream: if reply["stderr"].as_bool()? {
                Stream::Stderr
            } else {
                Stream::Stdout
            },
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
        let printed = match self.stream {
            Stream::Stdout if self.json => write_serialized_json(&self.body),
            Stream::Stdout => {
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
            }
            Stream::Stderr => {
                eprintln!("{}", self.body);
                // A failed row's restart and repair calls are its recovery.
                if let Some(notes) = &self.notes {
                    eprintln!("{notes}");
                }
                0
            }
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
