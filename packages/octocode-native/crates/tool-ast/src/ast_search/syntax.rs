use octocode_contract_types::{
    AstParser, AstSearchQuerySyntaxTreeContent, AstSearchQuerySyntaxTreePath, ErrorCode,
};
use octocode_engine::num::{saturating_u32, saturating_usize};
use octocode_engine::structural::SyntaxTreeInspectOptions;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::result::{ToolError, query_row};
use octocode_runtime_base::{execution::cancel::CancellationCheck, policy::path::PathPolicy};
use serde_json::{Value, json};

mod pricing;
mod render;
use render::render_tree_page;

/// Both generated source forms share the same reader and page renderer.
#[derive(Clone, Copy)]
pub(super) enum SyntaxQuery<'a> {
    File(&'a AstSearchQuerySyntaxTreePath),
    Snippet(&'a AstSearchQuerySyntaxTreeContent),
}

impl<'a> SyntaxQuery<'a> {
    fn parser(self) -> Option<&'a AstParser> {
        match self {
            Self::File(query) => query.parser.as_ref(),
            Self::Snippet(query) => Some(&query.parser),
        }
    }

    fn path(self) -> Option<&'a str> {
        match self {
            Self::File(query) => Some(query.path.as_str()),
            Self::Snippet(_) => None,
        }
    }

    fn page(self) -> u32 {
        saturating_u32(match self {
            Self::File(query) => query.page.get(),
            Self::Snippet(query) => query.page.get(),
        })
    }

    fn snapshot(self) -> Option<&'a str> {
        match self {
            Self::File(query) => query.snapshot.as_ref(),
            Self::Snippet(query) => query.snapshot.as_ref(),
        }
        .map(|snapshot| snapshot.as_str())
    }

    fn named_only(self) -> bool {
        match self {
            Self::File(query) => query.named_only,
            Self::Snippet(query) => query.named_only,
        }
    }

    /// Whether rows carry byte spans and origins (`includeProvenance`).
    fn provenance(self) -> bool {
        match self {
            Self::File(query) => query.include_provenance == Some(true),
            Self::Snippet(query) => query.include_provenance == Some(true),
        }
    }

    /// The 1-based line whose covering node roots the tree (`line`).
    fn line(self) -> Option<u32> {
        match self {
            Self::File(query) => query.line,
            Self::Snippet(query) => query.line,
        }
        .map(|line| saturating_u32(line.get()))
    }

    fn row(self, omit: &[&str]) -> Value {
        match self {
            Self::File(query) => query_row(query, omit),
            Self::Snippet(query) => query_row(query, omit),
        }
    }
}

pub(super) fn execute_syntax(
    q: SyntaxQuery<'_>,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    response_window: Option<usize>,
) -> Result<Value, ToolError> {
    run_syntax(q, paths, cancel, response_window)
        .map(octocode_runtime_base::response::result::finished)
}

/// `notAFile` for a `path` that names a directory says what syntaxTree
/// reads (one file) and leads to the listing of that directory, where the
/// file to pass is one row; any other failure is `error` as it is.
fn one_file_error(path: &str, paths: &PathPolicy, error: ToolError) -> ToolError {
    let is_directory = error.code == ErrorCode::NotAFile
        && paths
            .validate(path)
            .is_ok_and(|directory| directory.canonical.is_dir());
    if !is_directory {
        return error;
    }
    let mut error = ToolError::new(
        ErrorCode::NotAFile,
        format!(
            "syntaxTree reads one file, and `{}` is a directory: pass a file under it; operation:\"symbols\" outlines a directory.",
            octocode_runtime_base::security::quoted(path)
        ),
    );
    error.calls = octocode_tools_shared::leads::files_listing::scope_listing(
        path,
        Default::default(),
        false,
        false,
    )
    .map(|listing| Box::new(json!({ "viewTree": listing })));
    error
}

fn run_syntax(
    q: SyntaxQuery<'_>,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    response_window: Option<usize>,
) -> Result<Value, ToolError> {
    cancel.check().map_err(ToolError::cancelled)?;
    let language = q.parser().map(ToString::to_string);
    let validate_language = |source: &std::path::Path| {
        super::language::validate_file_language(
            source,
            language.as_deref(),
            octocode_engine::text::analysis::AnalysisOperation::SyntaxTree,
        )
    };
    // The source the tree is read from: a file, or the row's inline `content`,
    // which is parsed under its field's name and shows no path.
    let (canonical, display_path, bytes) = match q {
        SyntaxQuery::Snippet(query) => {
            let bytes = super::source::inline_source(query.content.as_str(), language.as_deref())?;
            let name = std::path::PathBuf::from(super::source::INLINE_NAME);
            validate_language(&name)?;
            (name, None, std::borrow::Cow::Borrowed(bytes))
        }
        SyntaxQuery::File(query) => {
            let path = query.path.as_str();
            let p = paths
                .validate_read(path)
                .map_err(|error| one_file_error(path, paths, super::errors::policy_error(error)))?;
            validate_language(&p.canonical)?;
            let display_path = super::RowPaths::new(paths, &p.canonical, true)
                .delivered(&p.canonical.to_string_lossy());
            let Some(bytes) = super::source::read_parse_source(&p.canonical)? else {
                return Ok(super::source::source_limit(
                    &p.canonical,
                    &display_path,
                    None,
                ));
            };
            (
                p.canonical,
                Some(display_path),
                std::borrow::Cow::Owned(bytes),
            )
        }
    };
    let canonical = canonical.as_path();
    // Node rows expose grammar labels and coordinates, never source text:
    // the coordinates of the text a read shows, as every view states them,
    // so no node's extent tells a hidden value's length.
    let cancelled = || cancel.check().is_err();
    let mut maps = super::source::mapping_budget(cancel, &cancelled);
    let (admitted, _) = super::source::admitted_for_match(canonical, &bytes, &mut maps)?;
    let content = admitted.text();
    let source_snapshot = octocode_runtime_base::digest::json_sha256(&json!([
        // Node-row shape version (1-based columns).
        "rows-v4",
        // A file's canonical path is absolute; inline source is its field's
        // name, and its text is the digest below.
        canonical.to_string_lossy(),
        q.parser(),
        octocode_engine::digest::sha256(&bytes),
        q.named_only(),
        q.line(),
        // Rows with and without provenance price and cut pages differently.
        q.provenance()
    ]));
    if let Some(line) = q.line() {
        check_line(q, line, content)?;
    }
    cancel.check().map_err(ToolError::cancelled)?;
    let source = Source {
        admitted: &admitted,
        content,
        path: canonical,
        extension: super::language::parser_override(canonical, language.as_deref()),
        node_chunk: NODE_CHUNK,
    };
    let budget = octocode_runtime_base::response::page_windows::page_budget(response_window);
    let page = read_page(q, &source, budget, &source_snapshot, &mut maps, cancel)?;
    let snapshot = view_snapshot(&source_snapshot, &page.r);
    // A page without a snapshot pins nothing: it pages the current tree, as
    // every astSearch and structureSearch listing does; only a snapshot
    // that no longer matches is stale.
    if q.page() > 1 && q.snapshot().is_some_and(|given| given != snapshot) {
        return Ok(octocode_runtime_base::response::result::stale_snapshot(
            octocode_runtime_base::response::result::restart_query(ToolId::AstSearch, &q.row(&[])),
        ));
    }
    let mut out = render_tree_page(q, display_path.as_deref(), &page, &snapshot);
    if let Some(warning) = admitted.warning() {
        octocode_runtime_base::response::paging::push_warnings(&mut out, [warning.to_owned()]);
    }
    if q.provenance() && page.r.status != "error" && !page.r.nodes.is_empty() {
        let bound = page
            .r
            .source_templates
            .bind(&admitted, &mut maps)
            .map_err(ToolError::source)?;
        let mut evidence = super::evidence::FileEvidence::new(&bound, &mut maps, cancel);
        let rows = out["nodes"].as_array_mut().ok_or_else(|| {
            ToolError::new(ErrorCode::ExecutionFailed, "Syntax rows are missing.")
        })?;
        for (row, node) in rows.iter_mut().zip(&page.r.nodes) {
            evidence.span(row, node.source_span, "sourceRange", "sourceOrigin", true)?;
        }
        let descriptors = evidence.finish()?;
        super::evidence::attach_page(&mut out, &descriptors, cancel)?;
    }
    Ok(out)
}

/// The observed extent is part of identity: a cold replay with a different
/// deadline census must restart rather than change a continuation's coverage.
fn view_snapshot(
    source: &str,
    result: &octocode_engine::structural::SyntaxTreeInspectResult,
) -> String {
    octocode_runtime_base::digest::json_sha256(&json!([
        "syntax-view-v1",
        source,
        result.total_nodes,
        result.status,
        result
            .diagnostics
            .iter()
            .map(|diagnostic| octocode_tools_shared::ast_rule::diagnostic_row(diagnostic, None))
            .collect::<Vec<_>>()
    ]))
}

/// A `line` past the file's last line is invalid input naming the line
/// count, repaired by the same tree without `line` (the whole file).
fn check_line(q: SyntaxQuery<'_>, line: u32, content: &str) -> Result<(), ToolError> {
    let total = content.lines().count();
    if saturating_usize(line) <= total.max(1) {
        return Ok(());
    }
    let mut error = ToolError::new(
        ErrorCode::InvalidInput,
        format!(
            "line {line} is past the end of the file ({}).",
            octocode_engine::text::counted(total, "line", "lines")
        ),
    );
    let repaired = q.row(&["line"]);
    error.calls = Some(Box::new(json!({
        "repair": octocode_runtime_base::response::result::restart_query(ToolId::AstSearch, &repaired)
    })));
    Err(error)
}

/// One read page: its nodes (and the walk's status and diagnostics), whether
/// a later page holds more, and its `pagination` facts.
struct TreePage {
    r: octocode_engine::structural::SyntaxTreeInspectResult,
    more: bool,
    facts: octocode_runtime_base::response::pages::PageFacts,
}

/// A window-sized walk kept for the pages after the first: the nodes read
/// so far and their page windows. The snapshot digests the file's content,
/// path, grammar and `namedOnly`, so an entry can never serve another
/// source; the page budget is its policy.
struct Walked {
    r: octocode_engine::structural::SyntaxTreeInspectResult,
    windows: Vec<std::ops::Range<usize>>,
    prices: pricing::Prices,
}

static WALKS: octocode_tools_shared::page_memo::PageMemo<std::sync::Arc<Walked>> =
    octocode_tools_shared::page_memo::PageMemo::new();

impl Walked {
    /// Rough heap bytes, the memo weight.
    fn weight(&self) -> usize {
        self.r
            .nodes
            .iter()
            .map(|node| std::mem::size_of_val(node) + node.kind.len())
            .sum::<usize>()
            + self.windows.len() * std::mem::size_of::<std::ops::Range<usize>>()
            + self.r.source_templates.heap_bytes()
            + self.prices.weight()
    }

    /// Whether the walk read past `page`'s window, or read every node it can.
    fn reaches(&self, page: usize) -> bool {
        self.windows.len() > page || self.r.next_offset.is_none()
    }
}

/// The parsed file a syntax-tree page is read from.
struct Source<'a> {
    admitted: &'a octocode_engine::text::analysis::AdmittedSource<'a>,
    content: &'a str,
    path: &'a std::path::Path,
    /// The grammar's extension when the request overrides the file's own.
    extension: Option<String>,
    /// Nodes one engine read takes: [`NODE_CHUNK`]; a test walks a small tree in
    /// chunks of its own size.
    node_chunk: u32,
}

#[cfg(test)]
thread_local! {
    static INSPECTIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Engine tree reads this thread made (tests).
#[cfg(test)]
pub(super) fn inspections() -> usize {
    INSPECTIONS.with(std::cell::Cell::get)
}

impl Source<'_> {
    /// The nodes `offset..offset + limit` of the tree, in preorder.
    fn inspect(
        &self,
        q: SyntaxQuery<'_>,
        offset: u32,
        limit: u32,
    ) -> Result<octocode_engine::structural::SyntaxTreeInspectResult, ToolError> {
        #[cfg(test)]
        INSPECTIONS.with(|count| count.set(count.get() + 1));
        Ok(octocode_engine::structural::inspect_with_extension(
            self.content,
            &self.path.to_string_lossy(),
            self.extension.as_deref(),
            Some(SyntaxTreeInspectOptions {
                named_only: Some(q.named_only()),
                node_offset: Some(offset),
                node_limit: Some(limit),
                line: q.line(),
            }),
        ))
    }
}

/// Page `q.page()` contains the nodes that fill the response window:
/// each page starts where the one before ended, so the walk reads nodes from
/// the start until the page's window is closed by a node that no longer
/// fits. That walk is kept by snapshot, so a later page reads on from it
/// instead of parsing and walking the file from its first node again.
fn read_page(
    q: SyntaxQuery<'_>,
    source: &Source<'_>,
    budget: usize,
    snapshot: &str,
    maps: &mut octocode_engine::text::analysis::MapBudget<'_>,
    cancel: &dyn CancellationCheck,
) -> Result<TreePage, ToolError> {
    use octocode_runtime_base::response::pages::PageFacts;
    let page = saturating_usize(q.page());
    let policy = budget.to_string();
    let stored = WALKS.get(snapshot, &policy, std::sync::Arc::clone);
    let walked = match stored {
        Some(walked) if walked.reaches(page) => walked,
        stored => {
            let (mut r, mut prices) = match stored {
                Some(walked) => (walked.r.clone(), walked.prices.clone()),
                None => (
                    source.inspect(q, 0, source.node_chunk)?,
                    pricing::Prices::default(),
                ),
            };
            let mut windows = priced_windows(q, &r, &mut prices, source, maps, budget, cancel)?;
            while let Some(next) = r.next_offset.filter(|_| windows.len() <= page) {
                cancel.check().map_err(ToolError::cancelled)?;
                let remaining = r.total_nodes.saturating_sub(next);
                let expected = remaining.min(source.node_chunk);
                let rest = source.inspect(q, next, expected)?;
                append_observed_window(&mut r, rest, next, expected)?;
                windows = priced_windows(q, &r, &mut prices, source, maps, budget, cancel)?;
            }
            let walked = std::sync::Arc::new(Walked { r, windows, prices });
            if walked.r.status != "error" {
                WALKS.put(
                    snapshot.to_owned(),
                    policy,
                    walked.weight(),
                    std::sync::Arc::clone(&walked),
                );
            }
            walked
        }
    };
    let windows = &walked.windows;
    let total = saturating_usize(walked.r.total_nodes);
    let more = windows.len() > page;
    let read_all = walked.r.next_offset.is_none();
    let window = windows
        .get(page.saturating_sub(1))
        .cloned()
        .unwrap_or(walked.r.nodes.len()..walked.r.nodes.len());
    let facts = PageFacts::open(window.start, window.len(), more)
        .with_total(total)
        .out_of_range(read_all && page > 1 && page > windows.len());
    let r = octocode_engine::structural::SyntaxTreeInspectResult {
        source_templates: walked.r.source_templates.clone(),
        nodes: walked.r.nodes[window].to_vec(),
        total_nodes: walked.r.total_nodes,
        next_offset: walked.r.next_offset,
        status: walked.r.status.clone(),
        diagnostics: walked.r.diagnostics.clone(),
    };
    Ok(TreePage { r, more, facts })
}

/// The page windows of a walk: a provenance row also pays for its origin and
/// the page's source descriptors; a plain row is its own line.
fn priced_windows(
    q: SyntaxQuery<'_>,
    result: &octocode_engine::structural::SyntaxTreeInspectResult,
    prices: &mut pricing::Prices,
    source: &Source<'_>,
    maps: &mut octocode_engine::text::analysis::MapBudget<'_>,
    budget: usize,
    cancel: &dyn CancellationCheck,
) -> Result<Vec<std::ops::Range<usize>>, ToolError> {
    if q.provenance() {
        pricing::windows(result, prices, source.admitted, maps, budget, cancel)
    } else {
        pricing::plain_windows(result, prices, budget, cancel)
    }
}

/// Replays stay within the first walk's observed census and coverage facts.
fn append_observed_window(
    target: &mut octocode_engine::structural::SyntaxTreeInspectResult,
    rest: octocode_engine::structural::SyntaxTreeInspectResult,
    start: u32,
    expected: u32,
) -> Result<(), ToolError> {
    if rest.nodes.len() != saturating_usize(expected)
        || rest.nodes.iter().enumerate().any(|(index, node)| {
            u32::try_from(index)
                .ok()
                .and_then(|index| start.checked_add(index))
                != Some(node.id)
        })
    {
        return Err(ToolError::new(
            ErrorCode::ScopeTooBroad,
            "The syntax-tree replay could not reproduce its already observed node window within the scan budget; narrow the tree with line.",
        ));
    }
    target.nodes.extend(rest.nodes);
    let end = start.saturating_add(expected);
    target.next_offset = (end < target.total_nodes).then_some(end);
    Ok(())
}

#[cfg(test)]
mod tests;

/// Nodes read from the engine per call while a window-sized page is cut.
const NODE_CHUNK: u32 = 100_000;
