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
