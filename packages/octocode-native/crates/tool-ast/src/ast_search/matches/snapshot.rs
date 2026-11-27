//! The snapshot that names a scan, and the checks a stored scan passes
//! before a continuation page reuses it.
use super::query::MatchQuery;
use super::types::{Scan, Scope};
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::response::result::ToolError;
use serde_json::json;

pub(super) fn directory_source_digest<'a>(
    files: impl Iterator<Item = (&'a str, Option<&'a str>)>,
) -> String {
    let identities = files.collect::<std::collections::BTreeMap<_, _>>();
    octocode_runtime_base::digest::json_sha256(&json!(identities))
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
        "retry": octocode_runtime_base::response::result::Continuation::continued_query(q.query()).build()
    })));
    Some(error)
}

/// Where a later window's stream starts: the rows the windows before it
/// listed, so its pages count from where the walk stands
/// (`pagination.offset` advances across windows), and, when the window
/// before ended inside the file this window reads first, how many of that
/// file's matches it listed (its stream starts after them).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct WindowStart {
    pub(super) before: u64,
    pub(super) carry: u64,
}

/// A later window's cursor state: its pin or snapshot, then after `~` the
/// rows before it, and after a second `~` its carry ([`WindowStart`]).
pub(super) fn counted(snapshot: String, start: WindowStart) -> String {
    match start {
        WindowStart {
            before: 0,
            carry: 0,
        } => snapshot,
        WindowStart { before, carry: 0 } => format!("{snapshot}~{before}"),
        WindowStart { before, carry } => format!("{snapshot}~{before}~{carry}"),
    }
}

/// The pin of a window that resumes inside the file the window before
/// ended in: its carry counts that file's matches as the build `build`
/// matched them, so a window of another build restarts instead of listing
/// that file's matches from two matchers.
pub(super) fn built(pin: &str, build: &str) -> String {
    let build = octocode_engine::digest::sha256(build.as_bytes());
    let tag = build.get(..16).unwrap_or(&build);
    format!("{pin}+{tag}")
}

/// [`counted`]'s pin or snapshot, and where its window's stream starts.
pub(super) fn uncounted(snapshot: &str) -> (&str, WindowStart) {
    let number = |text: &str| text.parse::<u64>().ok();
    let Some((rest, last)) = snapshot.rsplit_once('~') else {
        return (snapshot, WindowStart::default());
    };
    let Some(last) = number(last) else {
        return (snapshot, WindowStart::default());
    };
    match rest
        .rsplit_once('~')
        .and_then(|(bare, before)| Some((bare, number(before)?)))
    {
        Some((bare, before)) => (
            bare,
            WindowStart {
                before,
                carry: last,
            },
        ),
        None => (
            rest,
            WindowStart {
                before: last,
                carry: 0,
            },
        ),
    }
}

/// Where `q`'s window's stream starts ([`counted`]).
pub(super) fn listed_before(q: MatchQuery<'_>) -> WindowStart {
    q.snapshot()
        .filter(|_| q.scan_offset() > 0)
        .map_or_else(WindowStart::default, |snapshot| uncounted(&snapshot).1)
}

/// Fingerprint over the query shape, ordered result set and scanned source bytes.
/// Continuation cursors (page>1) that
/// carry a stale snapshot are rejected with `staleSnapshot`. Fields the
/// continuation normalizes to a default are digested by their effective
/// value so a fresh page matches. `build` is the identity of the build that
/// shapes the rows and cuts the pages (production passes
/// the facade's own); it is an input, so the digest is
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
        Some(before) => octocode_runtime_base::digest::json_sha256(&json!([bound, before])),
        None => octocode_runtime_base::digest::json_sha256(&bound),
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
