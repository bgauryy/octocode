//! Secret redaction of a scan's shown values, and its replay on later pages.

use super::*;
use crate::security::cut::strip_clip_markers;
use crate::tools::local_search::sources::Expected;
use octocode_contract_types::LocalSearchQuery;

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
/// the redacted rows and the record of this redaction.
pub(in crate::tools::local_search) fn redact_scan(
    scope: &PageScope<'_, impl CancellationCheck>,
    parsed: &mut octocode_engine::types::TextSearchResult,
    stored: Option<&Redaction>,
) -> Result<(Redacted, Redaction), ToolError> {
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
            return Ok((redacted, stored.clone()));
        }
    }
    let mut redacted = Redacted::new();
    let mut record = Redaction {
        output_root: output_root.to_path_buf(),
        ..Redaction::default()
    };
    // Files redact independently: on parallel workers, merged in file order
    // so the record (and the first failure) is the one a serial pass makes.
    let workers = crate::tools::workers::workers(scope.walk_threads, parsed.files.len(), None);
    let outcomes = crate::tools::workers::each_parallel(&mut parsed.files, workers, |file| {
        redact_file(scope, file)
    });
    for (file_index, outcome) in outcomes.into_iter().enumerate() {
        let FileRedaction {
            key_scan,
            changes,
            path,
        } = outcome?;
        if let Some(KeyScan { digest }) = key_scan {
            record.key_files.push((file_index, digest));
        }
        for (match_index, line, column, value) in changes {
            redacted.insert((path.clone(), line, column));
            record.changes.push((file_index, match_index, value));
        }
    }
    Ok((redacted, record))
}

/// What redacting one file changed.
struct FileRedaction {
    /// Its key-block scan, when its snippets looked like key material.
    key_scan: Option<KeyScan>,
    /// The values replaced: match index, line, column, new value.
    changes: Vec<(usize, u32, u32, String)>,
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
    visit_values(scope, file, 0..file.matches.len(), |_, hit| {
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
            .map(|bytes| <[u8; 32]>::from(Sha256::digest(bytes)));
        key_scan = Some(KeyScan { digest });
        if let (Some(digest), Expected::Stored(stored)) = (digest, expected_digest(&source_path))
            && stored != Some(digest)
        {
            return Err(changed_since_scan(query));
        }
        bytes
            .map(|bytes| {
                let text = String::from_utf8_lossy(&bytes).into_owned();
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
    visit_values(
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
                matched.value = sanitized.content;
                changed
            } else {
                let sanitized = security.sanitize_text(&matched.value, Some(&source_path));
                let changed = sanitized.content != matched.value;
                matched.value = sanitized.content;
                changed
            };
            if matched.deferred.is_some() {
                matched.deferred = None;
                let price = crate::tools::local_search::layout::row_chars(matched, None, false);
                matched.deferred = Some(u32_of_usize(price));
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
    )?;
    Ok(FileRedaction {
        key_scan,
        changes,
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
    for offset in 0..value_lines.len().max(1) as u32 {
        let Some(first) = line.checked_sub(offset).filter(|first| *first >= 1) else {
            break;
        };
        if !value_lines
            .iter()
            .enumerate()
            .all(|(i, value_line)| fits(value_line, first + i as u32))
        {
            continue;
        }
        aligned = true;
        for (i, flag) in redact.iter_mut().enumerate() {
            *flag |= in_block(first + i as u32);
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

/// Replay `stored` on the stored scan it was made from. Its key-block scans
/// read each file again, under the read policy and bound to the stored
/// digest exactly as a fresh redaction is (a changed file restarts the
/// snapshot). When each read hashes to the bytes the record was made from,
/// the key-block ranges, and so every replaced value, are the ones a fresh
/// redaction computes; `None` (no read, other bytes) redacts afresh.
fn replay_redaction(
    query: &LocalSearchQuery,
    parsed: &mut octocode_engine::types::TextSearchResult,
    output_root: &std::path::Path,
    expected_digest: &ExpectedDigest<'_>,
    paths: &crate::policy::path::PathPolicy,
    stored: &Redaction,
) -> Result<Option<Redacted>, ToolError> {
    for &(index, made_from) in &stored.key_files {
        let Some(file) = parsed.files.get(index) else {
            return Ok(None);
        };
        let source_path = output_root.join(&file.path);
        validate_matched(paths, &source_path)?;
        let Some(bytes) = read_key_scan_bytes(&source_path) else {
            return Ok(None);
        };
        let digest = <[u8; 32]>::from(Sha256::digest(&bytes));
        if let Expected::Stored(stored) = expected_digest(&source_path)
            && stored != Some(digest)
        {
            return Err(changed_since_scan(query));
        }
        if made_from != Some(digest) {
            return Ok(None);
        }
    }
    if stored.changes.iter().any(|&(file, row, _)| {
        parsed
            .files
            .get(file)
            .is_none_or(|file| row >= file.matches.len())
    }) {
        return Ok(None);
    }
    for file in &mut parsed.files {
        if let Ok(relative) = std::path::Path::new(&file.path).strip_prefix(output_root) {
            file.path = relative.to_string_lossy().into_owned();
        }
    }
    let mut redacted = Redacted::new();
    for (file, row, value) in &stored.changes {
        let file = &mut parsed.files[*file];
        let matched = &mut file.matches[*row];
        matched.value.clone_from(value);
        redacted.insert((file.path.clone(), matched.line, matched.column));
    }
    Ok(Some(redacted))
}
