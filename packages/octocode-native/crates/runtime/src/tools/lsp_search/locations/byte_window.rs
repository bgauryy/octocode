//! A declaration too long for its page (a minified bundle's one line) as a
//! byte window: it starts at the declaration and is cut from the redacted
//! view localFetch pages, so its end is a byte offset a localFetch
//! `readProgress` resumes at, and the window plus that walk are the rest of
//! the declaration's lines exactly once. The text before the declaration on
//! its first line is disclosed, with a related read of that line.
use super::shaping::{MAX_DECLARATION_CONTENT_LINES, public_location};
use super::*;
use crate::response::result::Continuation;
use crate::tools::source_view::local_fetch_query;
use octocode_contract_types::{LocalFetchQuery, ReadProgress, ReadProgressUnit};
use std::collections::HashMap;
use std::sync::Arc;

mod coordinates;

use coordinates::{line_span, raw_byte_of_column, view_offset};

/// The calls a windowed location's row carries: the read of the rest of its
/// declaration's lines (none when the window reached their end) and the
/// read of its first line from byte 0 (none when the window starts there).
pub(super) struct Windowed {
    pub(super) read: Option<Continuation>,
    pub(super) related: Option<Value>,
    /// Why no window of the redacted view could be cut: the row keeps its
    /// location, shows no source, and is partial for this cause.
    pub(super) unsealed: Option<Unsealed>,
}

/// Why a declaration's lines could not be cut from the view localFetch
/// serves: the partial reason and the cause in words.
#[derive(Clone, Debug)]
pub(super) struct Unsealed {
    pub(super) reason: crate::response::paging::PartialReason,
    pub(super) warning: String,
}

impl Unsealed {
    fn new(reason: crate::response::paging::PartialReason, warning: String) -> Self {
        Self { reason, warning }
    }
}

/// One file as localFetch serves it: the decoded text, the view it pages
/// (private-key blocks and secrets redacted, line count kept), and the
/// snapshot of its bytes.
struct Sealed {
    raw: String,
    view: String,
    snapshot: String,
}

/// The text localFetch cuts its pages from for a whole in-memory read
/// ([`crate::tools::source_view::prepare::sealed_view`]), beside the decoded
/// text the server's positions count.
fn seal(path: &std::path::Path) -> Result<Sealed, Unsealed> {
    use crate::response::paging::PartialReason;
    let bytes =
        octocode_engine::text::read_source(path, super::super::source::MAX_LSP_DIDOPEN_BYTES)
            .map_err(|error| {
                let reason = match error {
                    octocode_engine::text::ReadError::TooLarge(_) => PartialReason::FileTooLarge,
                    _ => PartialReason::DeclarationClipped,
                };
                Unsealed::new(
                    reason,
                    format!("the declaration's source could not be read: {error}"),
                )
            })?
            .bytes;
    let snapshot = octocode_engine::digest::sha256(&bytes);
    let view = crate::tools::source_view::prepare::sealed_view(
        &bytes,
        path,
        &crate::security::ContentSecurity::new(),
    )
    .map_err(|(_, message)| Unsealed::new(PartialReason::SecuritySelectedViewSizeLimit, message))?
    .text
    .into_owned();
    Ok(Sealed {
        raw: octocode_engine::text::decode(&bytes).text.into_owned(),
        view,
        snapshot,
    })
}

/// Window every entry whose declaration lines (as many as a row shows)
/// outgrow `budget`; entry `i`'s calls are at index `i`.
pub(super) async fn window_long_declarations(
    entries: &mut [Value],
    budget: usize,
    sources: &SourceCache<'_>,
) -> Vec<Option<Windowed>> {
    let mut sealed = HashMap::<String, Result<Arc<Sealed>, Unsealed>>::new();
    let mut windows = Vec::with_capacity(entries.len());
    for entry in entries.iter_mut() {
        let Some(target) = Target::of(entry, budget) else {
            windows.push(None);
            continue;
        };
        if !sealed.contains_key(&target.path) {
            let unread = |cause: String| {
                Unsealed::new(
                    crate::response::paging::PartialReason::DeclarationClipped,
                    cause,
                )
            };
            let source = match sources.policy().validate_read(&target.path) {
                Ok(validated) => tokio::task::spawn_blocking(move || seal(&validated.canonical))
                    .await
                    .unwrap_or_else(|error| {
                        Err(unread(format!("the source read task failed: {error}")))
                    })
                    .map(Arc::new),
                Err(error) => Err(unread(format!(
                    "the read policy refused the declaration's source: {error}"
                ))),
            };
            sealed.insert(target.path.clone(), source);
        }
        let window = match &sealed[&target.path] {
            Ok(source) => target.window(entry, source),
            Err(unsealed) => target.unsealed(entry, unsealed.clone()),
        };
        windows.push(window);
    }
    windows
}

/// A location whose shown declaration lines outgrow the page budget.
struct Target {
    path: String,
    /// One-based first and last line the row shows.
    first: u64,
    last: u64,
    /// One-based line the window starts on: the declaration's first line,
    /// or a contextLines row's anchor line.
    anchor: u64,
    /// Zero-based UTF-16 column of the window's start on `anchor`.
    column: usize,
    budget: usize,
}

impl Target {
    fn of(entry: &Value, budget: usize) -> Option<Self> {
        let content = entry.get("content").and_then(Value::as_str)?;
        if content.starts_with("[content unavailable") {
            return None;
        }
        let lines = content.split('\n').collect::<Vec<_>>();
        let shown = lines[..lines.len().min(MAX_DECLARATION_CONTENT_LINES)].join("\n");
        if row_units(entry, shown) <= budget {
            return None;
        }
        let first = entry.pointer("/displayRange/startLine")?.as_u64()?;
        // The row names its last line; content read with its line breaks
        // (a contextLines window) ends in one, which is no further line.
        let last = match entry
            .pointer("/displayRange/endLine")
            .and_then(Value::as_u64)
        {
            Some(last) => last.max(first),
            None => first + u64::try_from(lines.len()).ok()? - 1,
        };
        // A link names its declaration's start on the first line; otherwise
        // the window starts at the range (a contextLines row's anchor) when
        // the row shows its line, else at the first line's start.
        let (anchor, column) = entry
            .pointer("/displayRange/startCharacter")
            .and_then(Value::as_u64)
            .map(|character| (first, character.saturating_sub(1)))
            .or_else(|| {
                let line = entry.pointer("/range/start/line")?.as_u64()? + 1;
                (first..=last).contains(&line).then_some(())?;
                Some((line, entry.pointer("/range/start/character")?.as_u64()?))
            })
            .unwrap_or((first, 0));
        Some(Self {
            path: uri_to_path(entry.get("uri").and_then(Value::as_str)?),
            first,
            last,
            anchor,
            column: octocode_engine::num::saturating_usize(column),
            budget,
        })
    }

    /// Replace `entry`'s content with its window of `source`; `None` (and
    /// the entry untouched) when the source no longer holds its lines or
    /// the whole view of them fits.
    fn window(&self, entry: &mut Value, source: &Sealed) -> Option<Windowed> {
        let first = octocode_engine::num::saturating_usize(self.first.checked_sub(1)?);
        let last = octocode_engine::num::saturating_usize(self.last.checked_sub(1)?);
        let (Some(raw), Some(view)) = (
            line_span(&source.raw, first, last).and_then(|span| source.raw.get(span)),
            line_span(&source.view, first, last).and_then(|span| source.view.get(span)),
        ) else {
            let cause = format!(
                "the source no longer holds lines {}-{} the server named",
                self.first, self.last
            );
            let reason = crate::response::paging::PartialReason::DeclarationClipped;
            return self.unsealed(entry, Unsealed::new(reason, cause));
        };
        let anchor_line = raw
            .split_inclusive('\n')
            .take(octocode_engine::num::saturating_usize(
                self.anchor - self.first,
            ))
            .map(str::len)
            .sum::<usize>();
        let Some((start, start_line, end, window)) = (|| {
            let at = anchor_line + raw_byte_of_column(raw.get(anchor_line..)?, self.column);
            let window = crate::response::cut::window_at(
                view,
                view_offset(raw, view, at)?,
                self.room(entry, view.len()),
            );
            // The line the window starts on: the related read reaches it.
            let start_line =
                self.first + u64::try_from(view.get(..window.start)?.matches('\n').count()).ok()?;
            Some((window.start, start_line, window.end, view.get(window)?))
        })() else {
            return self.unsealed(
                entry,
                Unsealed::new(
                    crate::response::paging::PartialReason::DeclarationClipped,
                    "the declaration's source coordinates do not select a valid UTF-8 window"
                        .into(),
                ),
            );
        };
        if start == 0 && end == view.len() {
            return None;
        }
        let mut content = String::new();
        if start > 0 {
            content.push_str(&prefix_marker(start, start_line));
            content.push('\n');
        }
        content.push_str(window);
        let read = if end < view.len() {
            if !content.ends_with('\n') {
                content.push('\n');
            }
            content.push_str(&tail_marker(view.len() - end, end, self.first, self.last));
            Some(self.read_from(end, &source.snapshot)?)
        } else {
            None
        };
        let related = (start > 0)
            .then(|| local_fetch_query::ranges_lead(&self.path, &[(self.first, start_line)]))
            .flatten()
            .map(|line| Continuation::related_query(line).build());
        entry["content"] = json!(content);
        // The window's lines are numbered (and judged) from the line it
        // starts on, through the line it ends on.
        let shown = window.trim_end_matches('\n').matches('\n').count();
        entry["displayRange"]["startLine"] = json!(start_line);
        entry["displayRange"]["endLine"] = json!(start_line + u64::try_from(shown).ok()?);
        Some(Windowed {
            read,
            related,
            unsealed: None,
        })
    }

    /// The row of a declaration no window could be cut for: its location,
    /// one unnumbered line stating what was left out and why, and the
    /// localFetch read of its lines (which serves them as it serves any read).
    fn unsealed(&self, entry: &mut Value, unsealed: Unsealed) -> Option<Windowed> {
        let bytes = entry
            .get("content")
            .and_then(Value::as_str)
            .map_or(0, str::len);
        let read = local_fetch_query::ranges_lead(&self.path, &[(self.first, self.last)])
            .map(Continuation::continued_query);
        entry["content"] = json!(format!(
            "… {bytes} bytes omitted from source lines {}-{}: {}; readDeclaration reads them.",
            self.first, self.last, unsealed.warning
        ));
        Some(Windowed {
            read,
            related: None,
            unsealed: Some(unsealed),
        })
    }

    /// Escaped UTF-16 units the window text may take: the page budget less
    /// the row around it (both markers at their widest, and a line number for
    /// each line the window can hold).
    fn room(&self, entry: &Value, view_len: usize) -> usize {
        let markers = format!(
            "{}\n\n{}",
            prefix_marker(view_len, self.first),
            tail_marker(view_len, view_len, self.first, self.last)
        );
        let lines = usize::try_from(self.last - self.first)
            .unwrap_or(usize::MAX)
            .min(MAX_DECLARATION_CONTENT_LINES);
        let numbers = lines.saturating_mul(self.last.to_string().len() + 1);
        self.budget
            .saturating_sub(row_units(entry, markers))
            .saturating_sub(numbers)
            .max(1)
    }

    /// The localFetch read of the declaration's lines from view byte `end`,
    /// pinned to the bytes the window was cut from.
    fn read_from(&self, end: usize, snapshot: &str) -> Option<Continuation> {
        let mut read: LocalFetchQuery =
            local_fetch_query::ranges_lead(&self.path, &[(self.first, self.last)])?;
        local_fetch_query::set_progress(
            &mut read,
            Some(ReadProgress {
                unit: ReadProgressUnit::Bytes,
                offset: i64::try_from(end).ok()?,
            }),
        );
        local_fetch_query::set_snapshot(&mut read, Some(snapshot)).ok()?;
        Some(Continuation::continued_query(read))
    }
}

/// Units `entry` costs a page as a public row holding `content`.
fn row_units(entry: &Value, content: String) -> usize {
    let mut row = entry.clone();
    row["content"] = Value::String(content);
    octocode_engine::json_size::utf16_units(&public_location(row)).unwrap_or(usize::MAX)
}

/// The disclosure of the `bytes` before the declaration on line `line`.
fn prefix_marker(bytes: usize, line: u64) -> String {
    format!(
        "… {bytes} bytes omitted before the window on line {line}; the related localFetch reads them."
    )
}

/// The disclosure of the `bytes` after view byte `end` of lines
/// `first`-`last`.
fn tail_marker(bytes: usize, end: usize, first: u64, last: u64) -> String {
    format!(
        "… {bytes} bytes omitted after byte {end} of source lines {first}-{last}; readDeclaration reads them."
    )
}

#[cfg(test)]
mod tests;
