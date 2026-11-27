use super::*;
use octocode_contract_types::{ErrorCode, LocalFetchQuery};
use octocode_runtime_base::execution::cancel::{CancellationCheck, NeverCancel};
use octocode_runtime_base::security::scan::ContentScan;
use octocode_runtime_base::test_support::Fixture;
use octocode_test_support::json_fixture;
use octocode_tools_shared::source_view::*;
use sha2::Digest;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
pub(super) struct Paths(pub PathBuf);
impl PathAccess for Paths {
    fn validate_read(&self, p: &Path) -> Result<ValidatedRead, Box<PathFailure>> {
        let p = if p.is_absolute() {
            p.into()
        } else {
            self.0.join(p)
        };
        p.canonicalize()
            .map(|canonical| ValidatedRead {
                // As the workspace policy names it: relative to the root
                // when inside it.
                named: self
                    .0
                    .canonicalize()
                    .ok()
                    .and_then(|root| canonical.strip_prefix(root).ok().map(Path::to_path_buf))
                    .unwrap_or_else(|| canonical.clone())
                    .to_string_lossy()
                    .into_owned(),
                canonical,
                target: None,
                display: p.to_string_lossy().into_owned(),
                repeated: None,
            })
            .map_err(|error| {
                Box::new(PathFailure {
                    code: ErrorCode::FileAccessFailed,
                    message: error.to_string(),
                    safe_path: None,
                    resource_missing: false,
                    sparse_checkout: false,
                    clone_scope: None,
                    resolved: None,
                    meant: None,
                    not_a_file: None,
                    search_root: None,
                    view_tree: None,
                    corrected: None,
                    listing_entry: false,
                })
            })
    }
}
struct Safe;
impl ContentScan for Safe {
    fn sanitize(&self, text: &str, _: &Path) -> Result<(String, Vec<String>), (ErrorCode, String)> {
        Ok((
            text.replace("SECRET", "[REDACTED]"),
            if text.contains("SECRET") {
                vec!["Secrets detected and redacted: synthetic".into()]
            } else {
                vec![]
            },
        ))
    }
}
/// [`Safe`] judging sources at [`octocode_runtime_base::security::SourceLimits::SMALL`]: a
/// fixture past a limit is a fraction of a megabyte.
pub(super) struct SmallSafe;
impl ContentScan for SmallSafe {
    fn sanitize(
        &self,
        text: &str,
        path: &Path,
    ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
        Safe.sanitize(text, path)
    }

    fn limits(&self) -> octocode_runtime_base::security::SourceLimits {
        octocode_runtime_base::security::SourceLimits::SMALL
    }
}

/// Read `req` under `paths` with the redacting test scan, no cancellation,
/// default regex and no response window.
fn fetch(req: &LocalFetchQuery, paths: &impl PathAccess) -> LocalFetchResult {
    fetch_with(req, paths, &Safe)
}

/// [`fetch`] under the content scan `scan`.
pub(super) fn fetch_with(
    req: &LocalFetchQuery,
    paths: &impl PathAccess,
    scan: &impl octocode_runtime_base::security::scan::ContentScan,
) -> LocalFetchResult {
    execute_local_fetch(req, paths, scan, &NeverCancel, None, false)
}

pub(super) fn q(path: &Path) -> LocalFetchQuery {
    qj(path, serde_json::json!({}))
}

/// Walk `next.nextPage` from `query` to its end under the response `window`,
/// placing every page in `view` (the read's whole selected text): a byte page
/// at the offset it states, each match window it shows after a gap marker at
/// the byte the marker names, a line page at its first line. Each piece is
/// `view`'s bytes at its place and starts on a clean cut, and the pieces are
/// `view` exactly once, in whatever order the walk shows them. Returns each
/// page's place and text in walk order.
pub(super) fn walk_exactly_once(
    query: &LocalFetchQuery,
    paths: &impl PathAccess,
    window: Option<usize>,
    view: &str,
) -> Vec<(usize, String)> {
    walk_exactly_once_with(query, paths, &Safe, window, view)
}

/// [`walk_exactly_once`] under the content scan `scan`.
pub(super) fn walk_exactly_once_with(
    query: &LocalFetchQuery,
    paths: &impl PathAccess,
    scan: &impl ContentScan,
    window: Option<usize>,
    view: &str,
) -> Vec<(usize, String)> {
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(view.match_indices('\n').map(|(at, _)| at + 1))
        .collect();
    let mut query = query.clone();
    let (mut pages, mut placed) = (Vec::new(), Vec::new());
    loop {
        let read = execute_local_fetch(&query, paths, scan, &NeverCancel, window, false);
        assert_eq!(read.status, ReadStatus::Success, "{read:?}");
        let page = read.pagination.as_ref().expect("pagination");
        let at = match page.unit {
            octocode_contract_types::ReadProgressUnit::Bytes => page.offset,
            octocode_contract_types::ReadProgressUnit::Lines => line_starts[page.offset],
        };
        let text = read.content.clone().expect("content");
        assert!(!text.is_empty(), "every page makes progress: {page:?}");
        let mark = placed.len();
        // A selection's byte page names source bytes (its offset and its gap
        // markers alike): one line's shift from the view's.
        let shift = page.placed.map_or(0, |(source, _)| source - page.offset);
        for (at, piece) in
            octocode_tools_shared::source_view::pagination::placed_pieces(at + shift, &text)
        {
            let at = at - shift;
            assert_eq!(
                view.get(at..at + piece.len()),
                Some(piece),
                "the piece is the view's bytes at {at}"
            );
            assert!(
                octocode_runtime_base::response::cut::is_clean_cut(view, at)
                    && octocode_runtime_base::response::cut::is_clean_cut(view, at + piece.len()),
                "piece edges at {at}+{} are clean cuts",
                piece.len()
            );
            placed.push((at, piece.len()));
        }
        // The wire states the walk: `hasMore` exactly when a next call
        // exists.
        let wire = serde_json::to_value(&read).expect("row");
        // Q4: a page showing every match of a long line leaves the rest of
        // that line to an optional read of the whole line.
        let rest = read
            .warnings
            .iter()
            .any(|warning| warning.starts_with("Every match in line"))
            .then(|| read.next.as_ref().and_then(|next| next.read.clone()))
            .flatten();
        let next = read.next.and_then(|next| next.continue_window);
        assert_eq!(page.has_more, next.is_some(), "hasMore has its call");
        assert_eq!(
            wire["pagination"]["hasMore"],
            serde_json::json!(next.is_some()),
            "the wire's hasMore has its call: {}",
            wire["pagination"]
        );
        pages.push((at, text));
        assert!(pages.len() < 2_000, "the walk ends");
        if let Some(rest) = rest {
            // The page shows every match window of one long line: the rest
            // of that line is its optional read, whose walk is the line
            // once, this page's windows again among it. The required pages
            // go on after the line.
            let shown = placed.split_off(mark);
            let first = shown.iter().map(|(at, _)| *at).min().expect("a window");
            let line_start = view[..first].rfind('\n').map_or(0, |at| at + 1);
            let line_end = view[first..]
                .find('\n')
                .map_or(view.len(), |at| first + at + 1);
            assert!(
                shown
                    .iter()
                    .all(|(at, len)| *at >= line_start && at + len <= line_end),
                "the windows lie in one line"
            );
            pages.extend(walk_exactly_once_with(
                &rest.query,
                paths,
                scan,
                window,
                &view[line_start..line_end],
            ));
            placed.push((line_start, line_end - line_start));
        }
        let Some(next) = next else {
            break;
        };
        query = next.query;
    }
    placed.sort_unstable();
    let mut end = 0;
    for (at, len) in placed {
        assert_eq!(at, end, "no gap and no byte twice");
        end = at + len;
    }
    assert_eq!(end, view.len(), "the walk reaches the view's last byte");
    pages
}
fn numbered(n: usize) -> String {
    (1..=n).map(|i| format!("l{i}\n")).collect()
}
struct Cancel(AtomicUsize);
impl CancellationCheck for Cancel {
    fn check(&self) -> Result<(), String> {
        if self.0.fetch_add(1, Ordering::SeqCst) > 0 {
            Err("cancelled".into())
        } else {
            Ok(())
        }
    }
}
/// A query built from wire JSON (fields the test does not name default).
pub(crate) fn qj(path: &Path, fields: serde_json::Value) -> LocalFetchQuery {
    let mut query = serde_json::json!({"path": path.to_string_lossy()});
    for (key, value) in fields.as_object().expect("object") {
        query[key] = value.clone();
    }
    serde_json::from_value(query).expect("localFetch query")
}

pub(crate) fn amend(query: &mut LocalFetchQuery, changes: serde_json::Value) {
    *query = local_fetch_query::updated(query, changes).expect("canonical read variant");
}

fn workspace_policy(root: &Path) -> octocode_runtime_base::policy::path::PathPolicy {
    octocode_runtime_base::policy::path::PathPolicy::new(
        octocode_runtime_base::policy::path::PathPolicyConfig {
            workspace_root: Some(root.to_path_buf()),
            ..Default::default()
        },
    )
    .expect("policy")
}

/// `a.txt` of `lines` numbered lines where each line in `hits` reads
/// `hit<n>`, and a query matching `hit` with one context line.
fn hit_lines(t: &Fixture, lines: usize, hits: &[usize]) -> (Paths, LocalFetchQuery) {
    let p = t.0.join("a.txt");
    let text = hits.iter().fold(numbered(lines), |text, hit| {
        text.replace(&format!("l{hit}\n"), &format!("hit{hit}\n"))
    });
    fs::write(&p, text).expect("test fixture operation should succeed");
    let mut req = q(&p);
    amend(&mut req, json_fixture(r#"{"matchString": ["hit"]}"#));
    amend(
        &mut req,
        json_fixture(r#"{"contextLines": {"before": 1, "after": 1}}"#),
    );
    (Paths(t.0.clone()), req)
}

const PY_TWO: &str = "import os\n\ndef first(a):\n    x = 1\n    return a\n\n\ndef second(b):\n    if b:\n        return first(1)\n    return 2\n";

mod compact_literals;
mod continuations;
mod declarations;
mod key_formats;
mod known_formats;
mod limit;
mod line_walks;
mod long_lines;
mod match_misses;
mod match_patterns;
mod match_ranges;
mod match_windows;
mod next_page_walks;
mod paging;
mod paths;
mod read_facts;
mod redacted_windows;
mod redaction;
mod regex_dialects;
mod source;
mod stale_cursors;
mod streamed;
mod streamed_matches;
mod symbols;
mod views;
mod whole_walks;
mod wire;

/// One extracted document supplies the outline and its declaration spans.
#[test]
fn a_cold_symbols_view_seeds_the_declarations_from_its_own_parse() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join(format!("seeded_{}.rs", std::process::id()));
    let source = "pub fn first(value: usize) -> usize {\n    let doubled = value * 2;\n    doubled + 1\n}\n\npub struct Point;\n\nimpl Point {\n    pub fn origin() -> Self {\n        Point\n    }\n}\n";
    std::fs::write(&file, source).expect("source");
    let query = qj(&file, json_fixture(r#"{"view": "symbols"}"#));
    let out = fetch(&query, &workspace_policy(dir.path()));
    assert!(out.error.is_none(), "{out:?}");
    let content = out.content.as_deref().expect("outline");
    assert!(content.contains("1-4\tpub fn first"), "{content}");
    assert!(content.contains("9-11\t    pub fn origin"), "{content}");
    assert!(content.contains("pub struct Point"), "{content}");
}

/// The hit lines a page states: `matchedLines`, or, when it is absent because
/// every shown line is a hit, the lines `sourceLineRanges` locates.
pub(super) fn shown_hits(wire: &serde_json::Value) -> serde_json::Value {
    if let Some(lines) = wire.get("matchedLines") {
        return lines.clone();
    }
    let ranges = wire["sourceLineRanges"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    ranges
        .iter()
        .flat_map(|range| {
            range["line"].as_u64().unwrap_or(0)..=range["endLine"].as_u64().unwrap_or(0)
        })
        .collect::<Vec<_>>()
        .into()
}
