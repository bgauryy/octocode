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

/// The transport of a read (reached through `Deref`) and its body scanner.
pub(crate) struct Source<'a> {
    transport: &'a GitHubTransport,
    scan: &'a (dyn ContentScan + Sync),
    redacted: AtomicUsize,
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
        }
    }

    /// Placeholders this source put into bodies so far: the read's
    /// `redactedContent` warning counts them with the response walk's own.
    pub(super) fn redacted(&self) -> usize {
        self.redacted.load(Ordering::Relaxed)
    }

    /// Every `body` string under `value` (an item, or a list of them) redacted
    /// whole, by the scanner that guards the response.
    pub(super) fn redact_bodies(&self, value: &mut Value) -> Result<(), ProviderError> {
        match value {
            Value::Object(fields) => {
                for (key, child) in fields.iter_mut() {
                    match child {
                        Value::String(text) if key == "body" => self.redact(text)?,
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

    fn redact(&self, text: &mut String) -> Result<(), ProviderError> {
        let clean = sanitize_provider_text(self.scan, text, Path::new(SCAN_PATH))?;
        if clean != *text {
            let added = crate::security::placeholders(&clean)
                .saturating_sub(crate::security::placeholders(text));
            self.redacted.fetch_add(added.max(1), Ordering::Relaxed);
            *text = clean;
        }
        Ok(())
    }
}
