//! The provider seam of one history-item read: the transport and the scanner
//! that redacts every `body` GitHub returns, once, where it enters. Order is
//! redact -> reduce (`matchString`) -> paginate: a filter, a preview or a
//! character window sees only the redacted text, so a secret is never shown in
//! part by a window edge and a match never lands inside one.
use super::SCAN_PATH;
use crate::security::scan::{ContentScan, sanitize_provider_text};
use octocode_github::{GitHubTransport, ProviderError};
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The row's decision on whether the text of a changed file is withheld, by
/// its path and a rename's previous path.
pub(super) type Withholds<'a> = &'a (dyn Fn(&str, Option<&str>) -> bool + Sync);

/// The transport of a read (reached through `Deref`) and its body scanner.
pub(crate) struct Source<'a> {
    transport: &'a GitHubTransport,
    scan: &'a (dyn ContentScan + Sync),
    redacted: AtomicUsize,
    /// Whether the row's policy withholds the text of a file by its names
    /// (`path`, a rename's `previous`); `None`: the default policy.
    withholds: Option<Withholds<'a>>,
}

impl std::ops::Deref for Source<'_> {
    type Target = GitHubTransport;
    fn deref(&self) -> &GitHubTransport {
        self.transport
    }
}

impl<'a> Source<'a> {
    pub(super) fn new(transport: &'a GitHubTransport, scan: &'a (dyn ContentScan + Sync)) -> Self {
        Self {
            transport,
            scan,
            redacted: AtomicUsize::new(0),
            withholds: None,
        }
    }

    /// This source with the row's own decision on which files' patches it
    /// shows: the path policy of the call, which a user's opt-in lifts.
    pub(super) fn withholding(mut self, withholds: Withholds<'a>) -> Self {
        self.withholds = Some(withholds);
        self
    }

    /// Whether the patch of the changed file `file` (a REST file entry) is
    /// withheld: its name or a rename's old name is a credential file.
    fn withholds_patch(&self, file: &serde_json::Map<String, Value>) -> bool {
        let name = |key: &str| file.get(key).and_then(Value::as_str);
        let Some(path) = name("filename").or_else(|| name("path")) else {
            return false;
        };
        let previous = name("previous_filename");
        match self.withholds {
            Some(withholds) => withholds(path, previous),
            None => crate::policy::path::withheld_text_by_default(path, previous, None),
        }
    }

    /// Placeholders this source put into bodies so far, on any page and in
    /// files a filter leaves out: whether the read's `redactedContent`
    /// warning counts the placeholders its page shows.
    pub(super) fn redacted(&self) -> usize {
        self.redacted.load(Ordering::Relaxed)
    }

    /// Every `body` string under `value` (an item, or a list of them) redacted
    /// whole, by the scanner that guards the response, and every changed
    /// file's raw `patch` redacted as that file's text (its `filename` is the
    /// path, so the file's own rules apply) before any view numbers its lines:
    /// redaction reads source text, never formatted text.
    pub(super) fn redact_bodies(&self, value: &mut Value) -> Result<(), ProviderError> {
        match value {
            Value::Object(fields) => {
                // A credential file's patch is its text: it never enters, so
                // no later view, window or match can show it. The row keeps
                // its path, status and counts.
                if fields.get("patch").is_some_and(Value::is_string) && self.withholds_patch(fields)
                {
                    fields.remove("patch");
                    fields.insert("withheld".to_owned(), Value::Bool(true));
                }
                let filename = fields
                    .get("filename")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                for (key, child) in fields.iter_mut() {
                    match child {
                        Value::String(text) if key == "body" => self.redact(text, SCAN_PATH)?,
                        Value::String(text) if key == "patch" => {
                            self.redact(text, filename.as_deref().unwrap_or(SCAN_PATH))?;
                        }
                        Value::Object(_) | Value::Array(_) => self.redact_bodies(child)?,
                        _ => {}
                    }
                }
            }
            Value::Array(items) => {
                for item in items {
                    self.redact_bodies(item)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// `text`, the source text of `path`, redacted in place by the scanner
    /// that guards the response, with that path's rules; each placeholder it
    /// adds counts toward [`Self::redacted`].
    pub(super) fn redact(&self, text: &mut String, path: &str) -> Result<(), ProviderError> {
        let clean = sanitize_provider_text(self.scan, text, Path::new(path))?;
        if clean != *text {
            let added = crate::security::placeholders(&clean)
                .saturating_sub(crate::security::placeholders(text));
            self.redacted.fetch_add(added.max(1), Ordering::Relaxed);
            *text = clean;
        }
        Ok(())
    }
}
