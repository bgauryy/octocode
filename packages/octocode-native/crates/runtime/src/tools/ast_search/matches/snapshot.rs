//! The snapshot that names a scan, and the checks a stored scan passes
//! before a continuation page reuses it.
use super::query::MatchQuery;
use super::types::{Scan, Scope};
use crate::response::result::ToolError;
use octocode_contract_types::ErrorCode;
use serde_json::json;

pub(super) fn directory_source_digest<'a>(
    files: impl Iterator<Item = (&'a str, Option<&'a str>)>,
) -> String {
    let identities = files.collect::<std::collections::BTreeMap<_, _>>();
    crate::digest::json_sha256(&json!(identities))
}

#[cfg(unix)]
pub(super) fn memo_source_unchanged(_: &Scope, _: &Scan) -> bool {
    // The proof already checked change time and inode as well as size and mtime.
    true
}

#[cfg(not(unix))]
pub(super) fn memo_source_unchanged(scope: &Scope, scan: &Scan) -> bool {
    // Without Unix change time, restored mtime cannot prove source identity.
    // Reuse parsing only after bounded reads prove the saved source digest.
    let digests = scan
        .sources
        .iter()
        .map(|path| {
            let digest = super::super::source::read_parse_source(path)
                .ok()
                .flatten()
                .map(|bytes| octocode_engine::digest::sha256(&bytes));
            (path.to_string_lossy().into_owned(), digest)
        })
        .collect::<Vec<_>>();
    if scope.is_file {
        digests.first().and_then(|(_, digest)| digest.as_deref())
            == Some(scan.source_digest.as_str())
    } else {
        directory_source_digest(
            digests
                .iter()
                .map(|(path, digest)| (path.as_str(), digest.as_deref())),
        ) == scan.source_digest
    }
}

/// A continuation page whose fresh rescan stopped at a parse or match
/// deadline differs from its first page because the scan was cut, not
/// because the source changed: a retryable `timeout` with the same page as
/// `next.retry`, never a `staleSnapshot` that discards the pages read.
pub(super) fn interrupted_rescan(q: MatchQuery<'_>, scan: &Scan) -> Option<ToolError> {
    let interrupted = scan.files.iter().any(|file| {
        file.diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.code.as_str(),
                octocode_engine::structural::MATCH_DEADLINE
                    | octocode_engine::structural::PARSE_INTERRUPTED
            )
        })
    });
    if !interrupted {
        return None;
    }
    let mut error = ToolError::new(
        ErrorCode::Timeout,
        "The rescan for this page stopped at the structural match deadline, so it cannot be checked against the earlier pages; the source is unchanged.",
    );
    error.calls = Some(Box::new(json!({
        "retry": crate::response::result::Continuation::continued_query(q.query()).build()
    })));
    Some(error)
}

/// Fingerprint over the query shape, ordered result set and scanned source bytes.
/// Continuation cursors (page>1) that
/// carry a stale snapshot are rejected with `staleSnapshot`. Fields the
/// continuation normalizes to a default are digested by their effective
/// value so a fresh page matches. `build` is the identity of the build that
/// shapes the rows and cuts the pages (production passes
/// [`crate::build_identity::BUILD_IDENTITY`]); it is an input, so the digest is
/// a pure function of what it binds.
pub(super) fn match_snapshot_under(
    build: &str,
    q: MatchQuery<'_>,
    scope: &Scope,
    scan: &Scan,
) -> String {
    let ordered = scan
        .files
        .iter()
        .map(|f| (f.path.clone(), f.matches.len()))
        .collect::<Vec<_>>();
    let bound = json!([
        // The build that shaped the rows and cut the pages: a cursor of another
        // build restarts instead of mixing shapes across pages.
        build,
        scan_fields(q, scope),
        q.scan_offset(),
        &scan.source_digest,
        ordered
    ]);
    // A later window's pages answer for the windows before it too.
    match &scan.listed_before {
        Some(before) => crate::digest::json_sha256(&json!([bound, before])),
        None => crate::digest::json_sha256(&bound),
    }
}

/// The query fields that decide which files a scan evaluates, what it keeps
/// of their matches and their order. Fields the continuation normalizes to a
/// default are taken by their effective value.
pub(super) fn scan_fields(q: MatchQuery<'_>, scope: &Scope) -> serde_json::Value {
    json!([
        scope.canonical.to_string_lossy(),
        q.rule(),
        q.scope(),
        q.include(),
        q.exclude(),
        q.include_hidden(),
        q.include_ignored(),
        q.test_code(),
        q.max_depth(),
        scope.language,
        q.language_globs(),
        q.reverse(),
        // Always present: validation stamps the contract defaults.
        q.sort().unwrap_or_default(),
        q.result_view().as_deref().unwrap_or_default(),
    ])
}
