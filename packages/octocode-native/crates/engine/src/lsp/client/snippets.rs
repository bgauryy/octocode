//! Location snippets: the read policy, per-response source cache, and the
//! LSP range/position parsing that cuts snippet text.

use crate::error::{Error, Result};
use crate::lsp::types::{LspCodeSnippet, LspExactPosition, LspRange};
use crate::lsp::uri::uri_to_path;
use crate::text::LineIndex;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub(super) const MAX_SNIPPET_SOURCE_BYTES: u64 = crate::lsp::MAX_LSP_SOURCE_BYTES;
/// Snippet `content` for a location whose file the [`SnippetReadPolicy`]
/// refused. The location itself is kept so the caller can apply its own
/// policy to it; no byte of the file was read.
pub const SNIPPET_CONTENT_WITHHELD: &str = "[content withheld — path not authorized for reading]";

/// Decides which server-supplied location paths may be read for snippet
/// content. The authorizer receives the path decoded from the server's URI
/// and returns the path to read (for example its canonical, policy-validated
/// form), or `None` to refuse. It runs before any filesystem access to that
/// file. The default allows every path as given.
#[derive(Clone, Default)]
pub struct SnippetReadPolicy {
    authorizer: Option<Arc<SnippetPathAuthorizer>>,
    /// See [`Self::shared_reads`].
    shared: Option<Arc<Mutex<HashMap<String, CachedRead>>>>,
}

/// See [`SnippetReadPolicy`].
pub type SnippetPathAuthorizer = dyn Fn(&Path) -> Option<PathBuf> + Send + Sync;

impl SnippetReadPolicy {
    pub fn with_authorizer(
        authorizer: impl Fn(&Path) -> Option<PathBuf> + Send + Sync + 'static,
    ) -> Self {
        Self {
            authorizer: Some(Arc::new(authorizer)),
            shared: None,
        }
    }

    /// Share each path's authorization and read across every response read
    /// under this policy and its clones, so a file named by many responses
    /// is authorized, read, and line-indexed once. The reads live as long as
    /// the policy: build one per request, never per process, so a later
    /// request sees later edits.
    #[must_use]
    pub fn shared_reads(mut self) -> Self {
        self.shared = Some(Arc::default());
        self
    }

    fn shared_read(&self, path: &str) -> Option<CachedRead> {
        self.shared
            .as_ref()?
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(path)
            .cloned()
    }

    fn share_read(&self, path: &str, read: &CachedRead) {
        if let Some(shared) = self.shared.as_ref() {
            shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(path.to_owned(), read.clone());
        }
    }

    fn authorize(&self, path: &Path) -> Option<PathBuf> {
        match &self.authorizer {
            Some(authorizer) => authorizer(path),
            None => Some(path.to_path_buf()),
        }
    }
}

pub(super) async fn snippets_from_locations(
    value: &Value,
    policy: &SnippetReadPolicy,
) -> Result<Vec<LspCodeSnippet>> {
    let mut snippets = Vec::new();
    let mut content_cache = SnippetContentCache::new(policy.clone());
    match value {
        Value::Null => Ok(snippets),
        Value::Array(items) => {
            for item in items {
                if let Some(snippet) = snippet_from_location_like(item, &mut content_cache).await? {
                    snippets.push(snippet);
                }
            }
            Ok(snippets)
        }
        object @ Value::Object(_) => {
            if let Some(snippet) = snippet_from_location_like(object, &mut content_cache).await? {
                snippets.push(snippet);
            }
            Ok(snippets)
        }
        _ => Ok(snippets),
    }
}

/// One file's snippet source, split into lines once.
pub(super) struct CachedSource {
    pub(super) content: String,
    pub(super) lines: LineIndex,
}

#[derive(Clone)]
pub(super) enum CachedRead {
    Source(Arc<CachedSource>),
    Withheld,
    Failed(Error),
}

/// Per-response cache of snippet sources keyed by the server-supplied path,
/// so a file named by many locations is authorized, read, and line-indexed
/// once (failures and refusals are cached too); with
/// [`SnippetReadPolicy::shared_reads`], once per request.
pub(super) struct SnippetContentCache {
    policy: SnippetReadPolicy,
    pub(super) files: HashMap<String, CachedRead>,
    /// Decoded file path per location URI.
    paths: HashMap<String, String>,
}

impl SnippetContentCache {
    pub(super) fn new(policy: SnippetReadPolicy) -> Self {
        Self {
            policy,
            files: HashMap::new(),
            paths: HashMap::new(),
        }
    }

    /// The file path a location URI names, decoded once per response.
    pub(super) fn file_path(&mut self, uri: &str) -> Result<String> {
        if let Some(path) = self.paths.get(uri) {
            return Ok(path.clone());
        }
        let path = uri_to_path(uri)?;
        self.paths.insert(uri.to_owned(), path.clone());
        Ok(path)
    }

    /// Whole-line snippet for `range`, [`SNIPPET_CONTENT_WITHHELD`] when the
    /// policy refuses the path, or the read error.
    pub(super) async fn read_range_content(
        &mut self,
        file_path: &str,
        range: &LspRange,
    ) -> Result<String> {
        if !self.files.contains_key(file_path) {
            let entry = match self.policy.shared_read(file_path) {
                Some(shared) => shared,
                None => {
                    let entry = match self.policy.authorize(Path::new(file_path)) {
                        None => CachedRead::Withheld,
                        Some(authorized) => match read_snippet_source(authorized).await {
                            Ok(content) => CachedRead::Source(Arc::new(CachedSource {
                                lines: LineIndex::new(&content),
                                content,
                            })),
                            Err(error) => CachedRead::Failed(error),
                        },
                    };
                    self.policy.share_read(file_path, &entry);
                    entry
                }
            };
            self.files.insert(file_path.to_owned(), entry);
        }
        match self.files.get(file_path) {
            Some(CachedRead::Source(source)) => Ok(slice_range_content(source, range)),
            Some(CachedRead::Withheld) => Ok(SNIPPET_CONTENT_WITHHELD.to_owned()),
            Some(CachedRead::Failed(error)) => Err(error.clone()),
            None => Ok(String::new()),
        }
    }
}

async fn read_snippet_source(path: PathBuf) -> Result<String> {
    tokio::task::spawn_blocking(move || read_bounded_regular_file(&path, MAX_SNIPPET_SOURCE_BYTES))
        .await
        .map_err(|err| Error::new(format!("snippet read task failed: {err}")))?
}

/// The text of a regular file of at most `max_bytes`: read by
/// [`crate::text::read_source`], then admitted as UTF-8 or BOM-marked UTF-16
/// text ([`crate::text::admit`]).
pub(super) fn read_bounded_regular_file(path: &Path, max_bytes: u64) -> Result<String> {
    use crate::text::ReadError;
    let source = crate::text::read_source(path, max_bytes).map_err(|error| match error {
        ReadError::Io(err) => Error::new(err.to_string()),
        ReadError::NotRegular => {
            Error::new("not a regular file; LSP snippet content is read only from regular files")
        }
        ReadError::TooLarge(len) => Error::new(format!(
            "file too large for LSP snippet content ({len} bytes > {max_bytes} bytes)"
        )),
    })?;
    source
        .admit(crate::text::Decoding::Unicode)
        .map(|admitted| admitted.text.into_owned())
        .map_err(|refusal| Error::new(format!("file is not admitted as text: {refusal}")))
}

async fn snippet_from_location_like(
    value: &Value,
    content_cache: &mut SnippetContentCache,
) -> Result<Option<LspCodeSnippet>> {
    let uri = value
        .get("uri")
        .or_else(|| value.get("targetUri"))
        .and_then(Value::as_str);
    let range_value = value.get("range").or_else(|| value.get("targetRange"));
    let (Some(uri), Some(range_value)) = (uri, range_value) else {
        return Ok(None);
    };
    let context_range = parse_range(range_value)?;
    // LocationLink separates the symbol selection from its enclosing declaration.
    // Keep provider selection coordinates for navigation and enclosing source for context.
    let range = match value.get("targetSelectionRange") {
        Some(selection) => parse_range(selection)?,
        None => context_range.clone(),
    };
    let file_path = content_cache.file_path(uri)?;
    // A read failure here is real evidence ("target file is missing/unreadable/
    // generated"), not "no useful definition". Surface it as explicit content
    // instead of an empty string so callers don't misread it — and keep the
    // request resilient (one bad target must not drop the other locations).
    let content = match content_cache
        .read_range_content(&file_path, &context_range)
        .await
    {
        Ok(text) => text,
        Err(err) => format!("[content unavailable — could not read {uri}: {err}]"),
    };
    Ok(Some(LspCodeSnippet {
        uri: uri.to_owned(),
        range,
        content,
        symbol_kind: None,
        display_range: value.get("targetSelectionRange").map(|_| {
            json!({
                "startLine": context_range.start.line.saturating_add(1),
                "endLine": if context_range.end.character == 0 {
                    context_range
                        .end
                        .line
                        .max(context_range.start.line.saturating_add(1))
                } else {
                    context_range.end.line.saturating_add(1)
                }
            })
        }),
    }))
}

fn parse_range(value: &Value) -> Result<LspRange> {
    let start = value
        .get("start")
        .ok_or_else(|| Error::new("LSP range missing start"))?;
    let end = value
        .get("end")
        .ok_or_else(|| Error::new("LSP range missing end"))?;
    Ok(LspRange {
        start: parse_position(start)?,
        end: parse_position(end)?,
    })
}

pub(super) fn parse_position(value: &Value) -> Result<LspExactPosition> {
    // Required, numeric fields. Coercing a missing/malformed line or character
    // to 0 silently mis-positions snippets and masks protocol/server corruption,
    // so validate and surface InvalidArg instead.
    let line = value
        .get("line")
        .and_then(Value::as_u64)
        .ok_or_else(|| Error::new("LSP position missing numeric 'line'"))?;
    let character = value
        .get("character")
        .and_then(Value::as_u64)
        .ok_or_else(|| Error::new("LSP position missing numeric 'character'"))?;
    let out_of_range = |field: &str| Error::new(format!("LSP position '{field}' exceeds u32"));
    Ok(LspExactPosition {
        line: u32::try_from(line).map_err(|_| out_of_range("line"))?,
        character: u32::try_from(character).map_err(|_| out_of_range("character"))?,
    })
}

/// Slice `content` to an LSP range, returning whole lines for snippet context.
/// LSP ranges are **end-exclusive**: a range ending at `{line: N, character: 0}`
/// stops at the end of line `N-1` and must not include line `N`. When
/// `end.character > 0` the end line is partially covered, so it is included
/// (whole-line — column truncation would shrink a single-line definition
/// snippet down to the bare identifier).
///
/// Lines break on `\r\n`, `\n`, and lone `\r` (as the server counts them),
/// using the source's precomputed line index.
pub(super) fn slice_range_content(source: &CachedSource, range: &LspRange) -> String {
    let line_count = source.lines.content_len();
    let start = range.start.line as usize;
    if start >= line_count {
        return String::new();
    }
    let end = range.end.line as usize;
    let last_inclusive = if range.end.character == 0 {
        match end.checked_sub(1) {
            Some(v) => v,
            None => return String::new(),
        }
    } else {
        end
    };
    let last_inclusive = last_inclusive.min(line_count.saturating_sub(1));
    if last_inclusive < start {
        return String::new();
    }
    (start..=last_inclusive)
        .map(|line| source.lines.line(&source.content, line).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n")
}
