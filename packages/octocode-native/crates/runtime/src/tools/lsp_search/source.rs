//! Per-request source access for `lspSearch`: every file the request reads
//! (the anchor, definition-hop targets, alias-recovery candidates, and
//! `contextLines` windows) passes the read policy once, is read once through
//! a bounded regular-file read, and is line-indexed once.
//!
//! Server-supplied URIs are untrusted: nothing here reads a path the policy
//! refuses, and the snippet reads the engine performs for location requests
//! go through [`snippet_policy`], which applies the same policy *before* the
//! engine touches the file.

use super::render::decode_uri_path;
use crate::policy::path::PathPolicy;
use octocode_engine::lsp::client::SnippetReadPolicy;
use octocode_engine::text::LineIndex;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Upper bound on a source file synced to the language server via `didOpen`.
/// The engine's single LSP source bound: a document above this is oversize-diagnosed rather than read
/// uncapped and streamed to the server.
pub(super) const MAX_LSP_DIDOPEN_BYTES: u64 = octocode_engine::lsp::MAX_LSP_SOURCE_BYTES;

/// Why a bounded source read failed.
#[derive(Debug)]
pub(super) enum SourceReadError {
    TooLarge(u64),
    Unreadable(String),
}

/// Read a UTF-8 regular file of at most [`MAX_LSP_DIDOPEN_BYTES`] through the
/// engine's bounded regular-file read (non-blocking open, `fstat` of the
/// opened handle, capped read). Blocking: async callers use
/// [`read_bounded_source_async`].
pub(super) fn read_bounded_source(path: &Path) -> Result<String, SourceReadError> {
    use octocode_engine::lsp::BoundedRead;
    let bytes = octocode_engine::lsp::read_regular_bounded(path, MAX_LSP_DIDOPEN_BYTES).map_err(
        |error| match error {
            BoundedRead::TooLarge(len) => SourceReadError::TooLarge(len),
            BoundedRead::NotRegular => SourceReadError::Unreadable("not a regular file".into()),
            BoundedRead::Io(error) => SourceReadError::Unreadable(error.to_string()),
        },
    )?;
    String::from_utf8(bytes)
        .map_err(|error| SourceReadError::Unreadable(format!("not valid UTF-8: {error}")))
}

/// [`read_bounded_source`] on the blocking pool, so a slow filesystem never
/// stalls an async worker.
pub(super) async fn read_bounded_source_async(path: PathBuf) -> Result<String, SourceReadError> {
    tokio::task::spawn_blocking(move || read_bounded_source(&path))
        .await
        .unwrap_or_else(|error| {
            Err(SourceReadError::Unreadable(format!(
                "source read task failed: {error}"
            )))
        })
}

/// One read source plus its line index. Lines break on `\r\n`, `\n`, and a
/// lone `\r` (the LSP rule, via the engine's [`LineIndex`]); a final line
/// break does not start a counted line.
#[derive(Debug)]
pub(super) struct Source {
    pub(super) content: String,
    line_starts: Vec<usize>,
}

impl Source {
    pub(super) fn new(content: String) -> Self {
        let index = LineIndex::new(&content);
        let line_starts = if content.is_empty() {
            Vec::new()
        } else {
            (0..index.content_len())
                .filter_map(|line| index.line_start(line))
                .collect()
        };
        Self {
            content,
            line_starts,
        }
    }

    /// Number of content lines (a trailing line break adds none).
    pub(super) fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Text of lines `start..end` (0-based, end exclusive, clamped), each
    /// with its own line terminator.
    pub(super) fn lines(&self, start: usize, end: usize) -> &str {
        let end = end.min(self.line_count());
        if start >= end {
            return "";
        }
        let from = self.line_starts[start];
        let to = self
            .line_starts
            .get(end)
            .copied()
            .unwrap_or(self.content.len());
        &self.content[from..to]
    }
}

/// Request-scoped cache of policy decisions and file contents, keyed by the
/// path as the server (or query) named it.
///
/// Authorization and readability are separate: `authorized` records only
/// the read-policy decision, so an in-policy file whose content cannot be
/// loaded (too large, not UTF-8, unreadable) stays authorized and its
/// locations are kept (with the engine's "content unavailable" text) rather
/// than dropped as if the policy had refused them.
pub(super) struct SourceCache<'a> {
    paths: &'a PathPolicy,
    sources: HashMap<String, Option<Arc<Source>>>,
    authorized: HashMap<String, bool>,
    reads: usize,
}

impl<'a> SourceCache<'a> {
    pub(super) fn new(paths: &'a PathPolicy) -> Self {
        Self {
            paths,
            sources: HashMap::new(),
            authorized: HashMap::new(),
            reads: 0,
        }
    }

    pub(super) fn policy(&self) -> &'a PathPolicy {
        self.paths
    }

    /// Seed the cache with a source this request already read (the anchor
    /// document), so later windows reuse exactly the text sent in `didOpen`.
    pub(super) fn insert(&mut self, path: &str, content: String) -> Arc<Source> {
        let source = Arc::new(Source::new(content));
        self.sources
            .insert(path.to_owned(), Some(Arc::clone(&source)));
        self.authorized.insert(path.to_owned(), true);
        source
    }

    /// The policy-authorized, bounded source of `path`, read at most once
    /// per request (off the async worker). `None` when refused, unreadable,
    /// or oversize; only a policy refusal marks the path unauthorized.
    pub(super) async fn get(&mut self, path: &str) -> Option<Arc<Source>> {
        if let Some(cached) = self.sources.get(path) {
            return cached.clone();
        }
        let validated = self.paths.validate_read(path).ok();
        self.authorized
            .entry(path.to_owned())
            .or_insert(validated.is_some());
        let source = match validated {
            Some(validated) => {
                self.reads += 1;
                read_bounded_source_async(validated.canonical)
                    .await
                    .ok()
                    .map(|content| Arc::new(Source::new(content)))
            }
            None => None,
        };
        self.sources.insert(path.to_owned(), source.clone());
        source
    }

    /// Files actually read from disk by this cache (cache hits and policy
    /// refusals are not reads).
    pub(super) fn reads(&self) -> usize {
        self.reads
    }

    /// Whether a server-supplied file URI (or bare path) passes the read
    /// policy; memoized per URI. Whether its content was readable does not
    /// matter here.
    pub(super) fn uri_authorized(&mut self, uri: &str) -> bool {
        let Ok(path) = decode_uri_path(uri) else {
            return false;
        };
        if let Some(known) = self.authorized.get(&path) {
            return *known;
        }
        let allowed = self.paths.validate_read(&path).is_ok();
        self.authorized.insert(path, allowed);
        allowed
    }
}

/// The engine-side snippet read gate: a server-returned location path is
/// read only when it passes the request's read policy, and then through its
/// canonical, policy-validated form.
pub(super) fn snippet_policy(paths: &PathPolicy) -> SnippetReadPolicy {
    let policy = paths.clone();
    SnippetReadPolicy::with_authorizer(move |path| {
        policy
            .validate_read(path)
            .ok()
            .map(|validated| validated.canonical)
    })
}

/// Authorization gate for server-controlled items that embed a file URI in a
/// nested field — `WorkspaceSymbol`/`SymbolInformation` (`location.uri`),
/// call-hierarchy calls (`from.uri`/`to.uri`), type-hierarchy items (`uri`),
/// and location links (`targetUri`). A malicious or compromised server could
/// point these at files the caller never authorized; an item whose embedded
/// URI fails the read policy is dropped. An item with no embedded location
/// carries no file path to leak and is kept.
pub(super) fn item_uri_is_authorized(item: &Value, paths: &PathPolicy) -> bool {
    const URI_POINTERS: [&str; 5] = [
        "/uri",
        "/location/uri",
        "/from/uri",
        "/to/uri",
        "/targetUri",
    ];
    URI_POINTERS.iter().all(|pointer| {
        item.pointer(pointer)
            .and_then(Value::as_str)
            .is_none_or(|uri| {
                decode_uri_path(uri)
                    .ok()
                    .is_some_and(|path| paths.validate_read(path).is_ok())
            })
    })
}

/// Drop array entries whose embedded URI is not authorized. Non-array values
/// (e.g. `null`) pass through unchanged for the downstream `as_array` handling.
pub(super) fn filter_authorized_items(value: Value, paths: &PathPolicy) -> Value {
    match value {
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .filter(|item| item_uri_is_authorized(item, paths))
                .collect(),
        ),
        other => other,
    }
}
