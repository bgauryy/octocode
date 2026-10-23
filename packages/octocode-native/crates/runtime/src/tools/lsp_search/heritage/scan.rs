//! The syntax subtype scan, hedged against the language server, and how its
//! coverage is described.
use super::*;

/// What the heritage scan read: the subtypes of `base` among the `files`
/// that spell it, and the `unread` files (canonical paths) it could not read.
pub(in super::super) struct HeritageScan {
    pub(in super::super) base: String,
    pub(in super::super) files: usize,
    pub(in super::super) found: Vec<LspCodeSnippet>,
    pub(in super::super) unread: Vec<String>,
}

/// How long the server's `textDocument/implementation` answer may still
/// arrive once the syntax scan has finished: its rows are resolved, the
/// scan's are matched by name, so a server that is merely slow keeps the
/// win, but one that has not answered by then costs no further wait.
pub(in super::super) const SERVER_GRACE_AFTER_SCAN: std::time::Duration =
    std::time::Duration::from_secs(5);

/// Run `primary` (the server's request) and `hedge` (its syntax-scan
/// substitute) at the same time. A primary answer wins and drops the hedge,
/// which stops its blocking work, unless `keep` asks for the hedge beside
/// the answer: then it is awaited (or taken, finished) and rides with it. A
/// failure that `has_fallback` takes the hedge as it stands, finished or
/// not, so the scan's time overlaps the primary's instead of following it.
/// When the hedge finishes first and `grace` names a wait for it, the
/// primary has that long to answer before `overdue` (given the wait) turns
/// into its failure and the hedge is taken. The primary's own timeout still
/// bounds everything else.
pub(in super::super) async fn hedged<T, E, H>(
    primary: impl Future<Output = Result<T, E>>,
    hedge: impl Future<Output = H>,
    has_fallback: impl Fn(&E) -> bool,
    grace: impl Fn(&H) -> Option<std::time::Duration>,
    overdue: impl FnOnce(std::time::Duration) -> E,
    keep: bool,
) -> Result<(T, Option<H>), (E, Option<H>)> {
    let mut primary = std::pin::pin!(primary);
    let mut hedge = std::pin::pin!(hedge);
    let mut hedged = None;
    let mut waiting = std::pin::pin!(tokio::time::sleep(std::time::Duration::MAX));
    let mut allowed = None;
    let failure = loop {
        tokio::select! {
            biased;
            answer = &mut primary => match answer {
                Ok(answer) if !keep => return Ok((answer, None)),
                Ok(answer) => {
                    let hedged = match hedged {
                        Some(done) => done,
                        None => hedge.await,
                    };
                    return Ok((answer, Some(hedged)));
                }
                Err(failure) => break failure,
            },
            done = &mut hedge, if hedged.is_none() => {
                if let Some(wait) = grace(&done) {
                    waiting.as_mut().reset(tokio::time::Instant::now() + wait);
                    allowed = Some(wait);
                }
                hedged = Some(done);
            }
            () = &mut waiting, if allowed.is_some() => {
                break overdue(allowed.unwrap_or_default());
            }
        }
    };
    if !has_fallback(&failure) {
        return Err((failure, None));
    }
    let hedged = match hedged {
        Some(done) => done,
        None => hedge.await,
    };
    Err((failure, Some(hedged)))
}

/// The declarations of `content` whose `extends` / `implements` clause names
/// `base` (as written, or as the last segment of a dotted name), located at
/// the declaration's name. Matched by name, not resolved to a declaration.
pub(in super::super) fn subtype_snippets(
    content: &str,
    file: &str,
    base: &str,
) -> Vec<LspCodeSnippet> {
    let Some(facts) = octocode_engine::signatures::extract_graph_facts(content, file) else {
        return Vec::new();
    };
    let Ok(document) = serde_json::from_str::<GraphFactsDocument>(&facts) else {
        return Vec::new();
    };
    let Ok(uri) = octocode_engine::lsp::uri::path_to_uri(file) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    let mut seen = BTreeSet::new();
    for edge in document
        .edges
        .iter()
        .filter(|edge| matches!(edge.relation.as_str(), "extends" | "implements"))
        .filter(|edge| edge.to.rsplit(['.', ':']).next() == Some(base))
    {
        let Some(declaration) = document
            .declarations
            .iter()
            .find(|declaration| declaration.id == edge.from)
        else {
            continue;
        };
        if !seen.insert(declaration.id.as_str()) {
            continue;
        }
        let name = &declaration.selection_range;
        found.push(LspCodeSnippet {
            uri: uri.clone(),
            range: LspRange {
                start: LspExactPosition {
                    line: name.start.line,
                    character: name.start.character,
                },
                end: LspExactPosition {
                    line: name.end.line,
                    character: name.end.character,
                },
            },
            content: declaration.name.clone(),
            symbol_kind: None,
            display_range: None,
        });
    }
    found.sort_by_key(|snippet| (snippet.range.start.line, snippet.range.start.character));
    found
}

/// The subtypes of `base` among `files` (canonical paths of the files that
/// spell it), by file path then position, and the files that could not be
/// read, by path. Files are parsed in parallel; `stopped` abandons the scan.
pub(in super::super) fn scan_subtypes(
    files: &BTreeSet<String>,
    base: &str,
    stopped: &(dyn Fn() -> bool + Sync),
) -> (Vec<LspCodeSnippet>, Vec<String>) {
    let files = files.iter().collect::<Vec<_>>();
    let workers = crate::tools::workers::workers(None, files.len(), None);
    let chunk = files.len().div_ceil(workers).max(1);
    let (mut found, mut unread) = std::thread::scope(|scope| {
        let handles = files
            .chunks(chunk)
            .map(|files| {
                scope.spawn(move || {
                    let mut found = Vec::new();
                    let mut unread = Vec::new();
                    for file in files {
                        if stopped() {
                            break;
                        }
                        match read_bounded_source(std::path::Path::new(file)) {
                            Ok(content) => found.extend(subtype_snippets(&content, file, base)),
                            Err(_) => unread.push((*file).clone()),
                        }
                    }
                    (found, unread)
                })
            })
            .collect::<Vec<_>>();
        handles.into_iter().fold(
            (Vec::new(), Vec::new()),
            |(mut found, mut unread), handle| {
                let (more_found, more_unread) = handle.join().unwrap_or_default();
                found.extend(more_found);
                unread.extend(more_unread);
                (found, unread)
            },
        )
    });
    unread.sort();
    found.sort_by(|left, right| {
        (&left.uri, left.range.start.line, left.range.start.character).cmp(&(
            &right.uri,
            right.range.start.line,
            right.range.start.character,
        ))
    });
    (found, unread)
}

/// The warning about the files a heritage scan could not read: their
/// subtypes are missing from the answer. It names as many as the text-search
/// lead lists one row each and counts the rest; the lead reaches them all.
pub(in super::super) fn describe_unread(unread: &[String]) -> String {
    let named = unread.len().min(super::super::failure::LISTED_TEXT_FILES);
    let rest = match unread.len() - named {
        0 => String::new(),
        more => format!(" and {more} more"),
    };
    format!(
        "The heritage scan could not read {} file(s), so subtypes declared there are missing: {}{rest}.",
        unread.len(),
        unread[..named].join(", ")
    )
}

/// The caller-facing note of a heritage scan that replaced a failed
/// implementation request.
pub(in super::super) fn describe_scan(
    failure: &LspFailure,
    base: &str,
    files: usize,
    operation: &str,
) -> String {
    let noun = if operation == "implementation" {
        "implementations"
    } else {
        "subtypes"
    };
    format!(
        "The language server did not answer textDocument/implementation in time ({}): {noun} are the declarations whose extends/implements clause names `{base}`, read from source in the {files} files that spell it. They are direct {noun} only; run {operation} on each to descend. They are unverified: the base is matched by name, not resolved to this declaration. Retry asks the server again.",
        failure.message
    )
}
