//! Secret redaction of a scan's shown values, and its replay on later pages.

use super::*;
use crate::security::cut::strip_clip_markers;
use crate::tools::local_search::sources::Expected;

mod replay;

use replay::replay_redaction;

/// Make paths relative to `output_root` and redact secrets in every value.
/// A match on an interior base64 body line of a private key would leak the
/// key even though the match view holds no BEGIN/END marker (the anchored
/// built-in patterns need a complete block). Only when a snippet actually
/// looks like key material is the full file scanned for private-key block
/// ranges; matches whose window intersects a block are redacted. Innocent
/// base64 triggers a scan that finds no block and redacts nothing. A file
/// is read only after it passes the read policy.
///
/// `stored` is the redaction a fresh page made of this stored scan: it is
/// replayed when every key-block scan reads the same bytes again (see
/// [`replay_redaction`]); otherwise every value is redacted afresh. Returns
/// the redacted rows, the rows a fresh scan withheld because their file
/// changed during the call (a stored scan restarts instead), and the record
/// of this redaction.
pub(in crate::tools::local_search) fn redact_scan(
    scope: &PageScope<'_, impl CancellationCheck>,
    parsed: &mut octocode_engine::types::TextSearchResult,
    stored: Option<&Redaction>,
) -> Result<(Redacted, Withheld, Redaction), ToolError> {
    let PageScope {
        query,
        paths,
        output_root,
        expected_digest,
        cancel,
        ..
    } = *scope;
    if let Some(stored) = stored.filter(|stored| stored.output_root == output_root) {
        cancel.check().map_err(ToolError::cancelled)?;
        if let Some(redacted) =
            replay_redaction(query, parsed, output_root, expected_digest, paths, stored)?
        {
            return Ok((redacted, Withheld::new(), stored.clone()));
        }
    }
    let mut redacted = Redacted::new();
    let mut withheld = Withheld::new();
    let mut record = Redaction {
        output_root: output_root.to_path_buf(),
        ..Redaction::default()
    };
    // Files redact independently: on parallel workers, merged in file order
    // so the record (and the first failure) is the one a serial pass makes.
    let workers = crate::tools::workers::workers(parsed.files.len(), None);
    let fresh = stored.is_none();
    let outcomes = crate::tools::workers::each_parallel(&mut parsed.files, workers, |file| {
        if fresh && inspected_in_scan(file) {
            inspected_file(scope, file)
        } else {
            redact_file(scope, file)
        }
    });
    for (file_index, outcome) in outcomes.into_iter().enumerate() {
        let FileRedaction {
            key_scan,
            changes,
            held,
            path,
        } = outcome?;
        withheld.extend(
            held.into_iter()
                .map(|(line, column)| (path.clone(), line, column)),
        );
        if let Some(KeyScan { digest }) = key_scan {
            record.key_files.push((file_index, digest));
        }
        for (match_index, line, column, value) in changes {
            redacted.insert((path.clone(), line, column));
            record.changes.push((file_index, match_index, value));
        }
    }
    Ok((redacted, withheld, record))
}

/// The search's check of each deferred line, run while its bytes are in the
/// buffer so a page need not read the file again to redact a row it never
/// shows. It keeps what redaction changed and the row's exact price. A line
/// that may be key material stays unpriced: its file needs the whole-file
/// key-block scan of [`redact_file`].
pub(in crate::tools::local_search) fn scan_inspector() -> octocode_engine::types::DeferredInspector
{
    let security = ContentSecurity::new();
    octocode_engine::types::DeferredInspector(std::sync::Arc::new(move |path, hit| {
        if crate::security::snippet_may_hold_key_material(&hit.value) {
            hit.value = String::new();
            hit.deferred = Some(0);
            return;
        }
        let sanitized = security.sanitize_text(&hit.value, Some(path));
        let changed = sanitized.content != hit.value;
        if let std::borrow::Cow::Owned(text) = sanitized.content {
            hit.value = text;
        }
        hit.deferred = None;
        let price = crate::tools::local_search::layout::row_chars(hit, false);
        hit.deferred = Some(saturating_u32(price));
        if !changed {
            hit.value = String::new();
        }
    }))
}

/// Whether the scan already redacted and priced every value of `file`.
fn inspected_in_scan(file: &octocode_engine::types::TextSearchFile) -> bool {
    !file.matches.is_empty()
        && file
            .matches
            .iter()
            .all(|hit| hit.deferred.is_some_and(|price| price > 0))
}

/// The redaction [`scan_inspector`] already made of `file`: no source read.
fn inspected_file(
    scope: &PageScope<'_, impl CancellationCheck>,
    file: &mut octocode_engine::types::TextSearchFile,
) -> Result<FileRedaction, ToolError> {
    scope.cancel.check().map_err(ToolError::cancelled)?;
    if let Ok(relative) = std::path::Path::new(&file.path).strip_prefix(scope.output_root) {
        file.path = relative.to_string_lossy().into_owned();
    }
    Ok(FileRedaction {
        key_scan: None,
        changes: file
            .matches
            .iter()
            .enumerate()
            .filter(|(_, hit)| !hit.value.is_empty())
            .map(|(index, hit)| (index, hit.line, hit.column, hit.value.clone()))
            .collect(),
        held: Vec::new(),
        path: file.path.clone(),
    })
}

/// What redacting one file changed.
struct FileRedaction {
    /// Its key-block scan, when its snippets looked like key material.
    key_scan: Option<KeyScan>,
    /// The values replaced: match index, line, column, new value.
    changes: Vec<(usize, u32, u32, String)>,
    /// The rows withheld because the file changed during the call: line,
    /// column.
    held: Vec<(u32, u32)>,
    /// The file's row path.
    path: String,
}

/// A key-block scan of one file.
struct KeyScan {
    /// The digest of the bytes it read; `None` when the read failed.
    digest: Option<crate::tools::local_search::sources::Digest>,
}

/// Make one file's path relative to the output root and redact secrets in
/// its values (see [`redact_scan`]).
fn redact_file(
    scope: &PageScope<'_, impl CancellationCheck>,
    file: &mut octocode_engine::types::TextSearchFile,
) -> Result<FileRedaction, ToolError> {
    let PageScope {
        query,
        paths,
        output_root,
        expected_digest,
        security,
        ..
    } = *scope;
    if let Ok(relative) = std::path::Path::new(&file.path).strip_prefix(output_root) {
        file.path = relative.to_string_lossy().into_owned();
    }
    let mut key_material = false;
    let mut held = visit_values(scope, file, 0..file.matches.len(), |_, hit| {
        key_material |= crate::security::snippet_may_hold_key_material(&hit.value);
        if hit.deferred.is_some() {
            hit.value = String::new();
        }
        Ok(())
    })?;
    let source_path = output_root.join(&file.path);
    let mut key_scan = None;
    let key_source = if key_material {
        validate_matched(paths, &source_path)?;
        let bytes = read_key_scan_bytes(&source_path);
        let digest = bytes
            .as_ref()
            .map(|bytes| octocode_engine::digest::sha256_digest(bytes));
        key_scan = Some(KeyScan { digest });
        if let (Some(digest), Expected::Stored(stored)) = (digest, expected_digest(&source_path))
            && stored != Some(digest)
        {
            return Err(changed_since_scan(query));
        }
        bytes
            .map(|bytes| {
                // Decoded as the search decoded it.
                let text = octocode_engine::text::decode(&bytes).text.into_owned();
                (crate::security::private_key_block_line_ranges(&text), text)
            })
            .filter(|(ranges, _)| !ranges.is_empty())
    } else {
        None
    };
    let key_lines = key_source
        .as_ref()
        .map(|(ranges, text)| (ranges, text.lines().collect::<Vec<_>>()));
    let mut changes = Vec::new();
    held.extend(visit_values(
        scope,
        file,
        0..file.matches.len(),
        |match_index, matched| {
            let changed = if let Some((ranges, lines)) = &key_lines
                && crate::security::match_window_intersects_key_block(
                    matched.line,
                    &matched.value,
                    ranges,
                ) {
                let kept = redact_key_block_lines(matched.line, &matched.value, ranges, |n| {
                    lines.get((n as usize).checked_sub(1)?).copied()
                });
                let sanitized = security.sanitize_text(&kept, Some(&source_path));
                let changed = sanitized.content != matched.value;
                matched.value = match sanitized.content {
                    std::borrow::Cow::Borrowed(_) => kept,
                    std::borrow::Cow::Owned(text) => text,
                };
                changed
            } else {
                let sanitized = security.sanitize_text(&matched.value, Some(&source_path));
                let changed = sanitized.content != matched.value;
                if let std::borrow::Cow::Owned(text) = sanitized.content {
                    matched.value = text;
                }
                changed
            };
            if matched.deferred.is_some() {
                matched.deferred = None;
                let price = crate::tools::local_search::layout::row_chars(matched, false);
                matched.deferred = Some(saturating_u32(price));
                if !changed {
                    matched.value = String::new();
                }
            }
            if changed {
                changes.push((
                    match_index,
                    matched.line,
                    matched.column,
                    matched.value.clone(),
                ));
            }
            Ok(())
        },
    )?);
    // A withheld row's value came from bytes the scan did not read: none of
    // it is kept.
    changes.retain(|(index, ..)| !held.contains(index));
    Ok(FileRedaction {
        key_scan,
        changes,
        held: held
            .iter()
            .map(|&index| (file.matches[index].line, file.matches[index].column))
            .collect(),
        path: file.path.clone(),
    })
}

/// `value` (a match row whose `line` is the match) with each of its lines
/// that lies inside a private-key block replaced by the key placeholder,
/// so context lines beside a block stay readable. The value's first source
/// line is found by aligning its lines with `source` (1-based lines); a
/// line is redacted when any alignment puts it inside a block. A value no
/// alignment fits is redacted whole: fail closed.
pub(super) fn redact_key_block_lines<'a>(
    line: u32,
    value: &str,
    ranges: &[(u32, u32)],
    source: impl Fn(u32) -> Option<&'a str>,
) -> String {
    let placeholder = crate::security::key_fragment_placeholder();
    let value_lines: Vec<&str> = value.lines().collect();
    let in_block = |n: u32| ranges.iter().any(|&(s, e)| s <= n && n <= e);
    // A line a sanitizer already redacted fits where its unredacted
    // segments do.
    let fits = |value_line: &str, n: u32| {
        let core = strip_clip_markers(value_line).trim();
        source(n).is_some_and(|source| {
            octocode_engine::security::placeholder_regex()
                .split(core)
                .map(str::trim)
                .all(|segment| segment.is_empty() || source.contains(segment))
        })
    };
    let mut redact = vec![false; value_lines.len()];
    let mut aligned = false;
    for offset in 0..saturating_u32(value_lines.len().max(1)) {
        let Some(first) = line.checked_sub(offset).filter(|first| *first >= 1) else {
            break;
        };
        if !value_lines
            .iter()
            .enumerate()
            .all(|(i, value_line)| fits(value_line, first.saturating_add(saturating_u32(i))))
        {
            continue;
        }
        aligned = true;
        for (i, flag) in redact.iter_mut().enumerate() {
            *flag |= in_block(first.saturating_add(saturating_u32(i)));
        }
    }
    if !aligned {
        return placeholder;
    }
    let mut out = value_lines
        .iter()
        .zip(&redact)
        .map(|(value_line, redact)| {
            if *redact {
                placeholder.as_str()
            } else {
                value_line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if value.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// The bytes a key-block scan reads: the whole regular file within
/// [`MAX_KEY_SCAN_BYTES`], or `None` (also for a FIFO or link swapped in
/// since the walk).
pub(in crate::tools::local_search) fn read_key_scan_bytes(
    source: &std::path::Path,
) -> Option<Vec<u8>> {
    octocode_engine::text::read_regular(source, MAX_KEY_SCAN_BYTES)
        .ok()
        .map(|source| source.bytes)
}
