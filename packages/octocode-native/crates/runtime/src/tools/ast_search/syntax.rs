use crate::id::ToolId;
use crate::num::usize_of;
use crate::response::result::ToolError;
use crate::{
    execution::cancel::CancellationCheck, policy::path::PathPolicy, security::ContentSecurity,
};
use octocode_contract_types::{AstSearchQuerySyntaxTree, ErrorCode};
use octocode_engine::structural::SyntaxTreeInspectOptions;
use serde_json::{Value, json};

mod render;
use render::{node_page_chars, render_tree_page};

/// Engine-unit views over the generated `syntaxTree` query.
pub(crate) mod ast_search_query_syntax_tree {
    use crate::num::u32_of;
    use octocode_contract_types::AstSearchQuerySyntaxTree;
    pub(crate) fn language(query: &AstSearchQuerySyntaxTree) -> Option<String> {
        query.parser.as_ref().map(ToString::to_string)
    }
    pub(crate) fn page(query: &AstSearchQuerySyntaxTree) -> u32 {
        u32_of(query.page.get())
    }
    pub(crate) fn snapshot(query: &AstSearchQuerySyntaxTree) -> Option<&str> {
        query.snapshot.as_deref().map(String::as_str)
    }
    /// The 1-based line whose covering node roots the tree (`line`).
    pub(crate) fn line(query: &AstSearchQuerySyntaxTree) -> Option<u32> {
        query.line.map(|line| u32_of(line.get()))
    }
}

pub fn execute_syntax(
    q: &AstSearchQuerySyntaxTree,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
    response_window: Option<usize>,
) -> Result<Value, ToolError> {
    run_syntax(q, paths, security, cancel, response_window).map(super::output::finish)
}

fn run_syntax(
    q: &AstSearchQuerySyntaxTree,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
    response_window: Option<usize>,
) -> Result<Value, ToolError> {
    cancel.check().map_err(ToolError::cancelled)?;
    let p = paths
        .validate_read(q.path.as_str())
        .map_err(super::errors::policy_error)?;
    super::language::validate_file_language(
        &p.canonical,
        ast_search_query_syntax_tree::language(q).as_deref(),
    )?;
    let display_path =
        super::RowPaths::new(paths, &p.canonical, true).delivered(&p.canonical.to_string_lossy());
    let Some(bytes) = super::source::read_parse_source(&p.canonical)? else {
        return Ok(super::source::source_limit(&display_path, None));
    };
    let sanitized = security.sanitize_text(
        &super::source::parse_text(&p.canonical, &bytes)?,
        Some(&p.canonical),
    );
    let snapshot = crate::digest::json_sha256(&json!([
        // Node-row shape version (1-based columns).
        "rows-v2",
        p.canonical.to_string_lossy(),
        q.parser,
        sanitized.content,
        q.named_only,
        ast_search_query_syntax_tree::line(q)
    ]));
    // A page without a snapshot pins nothing: it pages the current tree, as
    // every astSearch and structureSearch listing does; only a snapshot
    // that no longer matches is stale.
    if ast_search_query_syntax_tree::page(q) > 1
        && ast_search_query_syntax_tree::snapshot(q).is_some_and(|given| given != snapshot)
    {
        return Ok(crate::response::result::stale_snapshot(
            crate::response::result::restart_query(
                ToolId::AstSearch,
                &crate::response::result::query_row(q, &[]),
            ),
        ));
    }
    if let Some(line) = ast_search_query_syntax_tree::line(q) {
        check_line(q, line, &sanitized.content)?;
    }
    cancel.check().map_err(ToolError::cancelled)?;
    let source = Source {
        content: &sanitized.content,
        path: &p.canonical,
        extension: super::language::parser_override(
            &p.canonical,
            ast_search_query_syntax_tree::language(q).as_deref(),
        ),
    };
    let budget = crate::response::page_windows::page_budget(
        response_window,
        &crate::response::result::query_value(q),
    );
    let page = read_page(q, &source, budget, &snapshot, cancel)?;
    Ok(render_tree_page(q, &display_path, page, &snapshot))
}

/// A `line` past the file's last line is invalid input naming the line
/// count, repaired by the same tree without `line` (the whole file).
fn check_line(q: &AstSearchQuerySyntaxTree, line: u32, content: &str) -> Result<(), ToolError> {
    let total = content.lines().count();
    if usize_of(line) <= total.max(1) {
        return Ok(());
    }
    let mut error = ToolError::new(
        ErrorCode::InvalidInput,
        format!("line {line} is past the end of the file ({total} lines)."),
    );
    let repaired = crate::response::result::query_row(q, &["line"]);
    error.calls = Some(Box::new(json!({
        "repair": crate::response::result::restart_query(ToolId::AstSearch, &repaired)
    })));
    Err(error)
}

/// One read page: its nodes (and the walk's status and diagnostics), whether
/// a later page holds more, and its `pagination` facts.
struct TreePage {
    r: octocode_engine::structural::SyntaxTreeInspectResult,
    more: bool,
    facts: crate::response::pages::PageFacts,
}

/// A window-sized walk kept for the pages after the first: the nodes read
/// so far and their page windows. The snapshot digests the file's content,
/// path, grammar and `namedOnly`, so an entry can never serve another
/// source; the page budget is its policy.
struct Walked {
    r: octocode_engine::structural::SyntaxTreeInspectResult,
    windows: Vec<std::ops::Range<usize>>,
}

static WALKS: crate::tools::page_memo::PageMemo<std::sync::Arc<Walked>> =
    crate::tools::page_memo::PageMemo::new(std::time::Duration::from_secs(60), 8, 64 << 20);

impl Walked {
    /// Rough heap bytes, the memo weight.
    fn weight(&self) -> usize {
        self.r
            .nodes
            .iter()
            .map(|node| std::mem::size_of_val(node) + node.kind.len())
            .sum::<usize>()
            + self.windows.len() * std::mem::size_of::<std::ops::Range<usize>>()
    }

    /// Whether the walk read past `page`'s window, or read every node it can.
    fn reaches(&self, page: usize) -> bool {
        self.windows.len() > page || self.r.next_offset.is_none() || self.r.status != "ok"
    }
}

/// The parsed file a syntax-tree page is read from.
struct Source<'a> {
    content: &'a str,
    path: &'a std::path::Path,
    /// The grammar's extension when the request overrides the file's own.
    extension: Option<String>,
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
        q: &AstSearchQuerySyntaxTree,
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
                named_only: Some(q.named_only),
                node_offset: Some(offset),
                node_limit: Some(limit),
                line: ast_search_query_syntax_tree::line(q),
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
    q: &AstSearchQuerySyntaxTree,
    source: &Source<'_>,
    budget: usize,
    snapshot: &str,
    cancel: &dyn CancellationCheck,
) -> Result<TreePage, ToolError> {
    use crate::response::pages::PageFacts;
    let page = usize_of(ast_search_query_syntax_tree::page(q));
    let policy = budget.to_string();
    let stored = WALKS.get(snapshot, &policy, std::sync::Arc::clone);
    let walked = match stored {
        Some(walked) if walked.reaches(page) => walked,
        stored => {
            let mut r = match stored {
                Some(walked) => walked.r.clone(),
                None => source.inspect(q, 0, NODE_CHUNK)?,
            };
            let mut windows = page_windows(&r, budget);
            while let Some(next) = r
                .next_offset
                .filter(|_| windows.len() <= page && r.status == "ok")
            {
                cancel.check().map_err(ToolError::cancelled)?;
                let rest = source.inspect(q, next, NODE_CHUNK)?;
                r.nodes.extend(rest.nodes);
                r.next_offset = rest.next_offset;
                r.status = rest.status;
                for diagnostic in rest.diagnostics {
                    if !r
                        .diagnostics
                        .iter()
                        .any(|seen| seen.code == diagnostic.code)
                    {
                        r.diagnostics.push(diagnostic);
                    }
                }
                windows = page_windows(&r, budget);
            }
            let walked = std::sync::Arc::new(Walked { r, windows });
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
    let total = usize_of(walked.r.total_nodes);
    let more = windows.len() > page;
    let read_all = walked.r.next_offset.is_none() || walked.r.status != "ok";
    let facts = if read_all {
        PageFacts::sized(page, windows.len(), total, more)
            .out_of_range(page > 1 && page > windows.len())
    } else {
        PageFacts::open(page, None, more).with_total(total)
    };
    let window = windows
        .get(page.saturating_sub(1))
        .cloned()
        .unwrap_or(walked.r.nodes.len()..walked.r.nodes.len());
    let r = octocode_engine::structural::SyntaxTreeInspectResult {
        nodes: walked.r.nodes[window].to_vec(),
        total_nodes: walked.r.total_nodes,
        next_offset: walked.r.next_offset,
        status: walked.r.status.clone(),
        diagnostics: walked.r.diagnostics.clone(),
    };
    Ok(TreePage { r, more, facts })
}

/// The page windows of the nodes a walk read, under `budget`.
fn page_windows(
    r: &octocode_engine::structural::SyntaxTreeInspectResult,
    budget: usize,
) -> Vec<std::ops::Range<usize>> {
    let sizes = r.nodes.iter().map(node_page_chars).collect::<Vec<_>>();
    crate::response::page_windows::Paging::of(None, budget).windows(&sizes)
}

/// Nodes read from the engine per call while a window-sized page is cut.
const NODE_CHUNK: u32 = 100_000;
