//! Replay of a stored redaction on the later pages of its scan.

use super::*;
use octocode_contract_types::LocalSearchQuery;

/// Replay `stored` on the stored scan it was made from. Its key-block scans
/// read each file again, under the read policy and bound to the stored
/// digest exactly as a fresh redaction is (a changed file restarts the
/// snapshot). When each read hashes to the bytes the record was made from,
/// the key-block ranges, and so every replaced value, are the ones a fresh
/// redaction computes; `None` (no read, other bytes) redacts afresh.
pub(super) fn replay_redaction(
    query: &LocalSearchQuery,
    parsed: &mut octocode_engine::types::TextSearchResult,
    (output_root, key_scan): (&std::path::Path, u64),
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
        let Some(bytes) = read_key_scan_bytes(&source_path, key_scan) else {
            return Ok(None);
        };
        let digest = octocode_engine::digest::sha256_digest(&bytes);
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

/// The key-block scans of a seekable page's files (`key_scans`, by index
/// into `files`), read again under the read policy and bound to the stored
/// digest as a fresh redaction reads them: a changed file restarts the
/// snapshot. What the scans redacted is in the page's frames, so a read
/// that fails redacts nothing afresh; the page's verification still reads
/// and checks each file it shows.
pub(in crate::tools::local_search) fn check_key_scans(
    scope: &PageScope<'_, impl CancellationCheck>,
    files: &[octocode_engine::types::TextSearchFile],
    key_scans: &[usize],
) -> Result<(), ToolError> {
    for file in key_scans.iter().filter_map(|index| files.get(*index)) {
        scope.cancel.check().map_err(ToolError::cancelled)?;
        let source = scope.output_root.join(&file.path);
        validate_matched(scope.paths, &source)?;
        let Some(bytes) = read_key_scan_bytes(&source, scope.security.limits().key_scan) else {
            continue;
        };
        if let Expected::Stored(stored) = (scope.expected_digest)(&source)
            && stored != Some(octocode_engine::digest::sha256_digest(&bytes))
        {
            return Err(changed_since_scan(scope.query));
        }
    }
    Ok(())
}
