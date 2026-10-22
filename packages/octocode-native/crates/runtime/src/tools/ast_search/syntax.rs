use crate::id::ToolId;
use crate::num::{u32_of, usize_of};
use crate::response::result::Confidence;
use crate::response::result::ToolError;
use crate::{
    execution::cancel::CancellationCheck, policy::path::PathPolicy, security::ContentSecurity,
};
use octocode_engine::structural::SyntaxTreeInspectOptions;
use serde_json::json;
const MAX_SOURCE: usize = super::MAX_PARSE_SOURCE_BYTES;
pub use octocode_contract_types::AstSearchQuerySyntaxTree;

/// Engine-unit views over the generated `syntaxTree` query.
pub(crate) mod ast_search_query_syntax_tree {
    use super::*;
    pub(crate) fn language(query: &AstSearchQuerySyntaxTree) -> Option<String> {
        query.language.as_ref().map(ToString::to_string)
    }
    pub(crate) fn page(query: &AstSearchQuerySyntaxTree) -> u32 {
        u32_of(query.page.get())
    }
    /// The caller's nodes per page; unset fills the response window.
    pub(crate) fn page_size(query: &AstSearchQuerySyntaxTree) -> Option<u32> {
        query.page_size.map(|size| u32_of(size.get()))
    }
    /// The first node of an explicit-size page; a window-sized page starts
    /// where the previous one ended, which only a walk of the nodes knows.
    pub(crate) fn node_offset(query: &AstSearchQuerySyntaxTree) -> Option<u32> {
        page_size(query).map(|size| page(query).saturating_sub(1).saturating_mul(size))
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
) -> super::AstResult {
    cancel.check().map_err(ToolError::cancelled)?;
    let p = paths
        .validate_read(q.path.as_str())
        .map_err(super::policy_error)?;
    super::validate_file_language(
        &p.canonical,
        ast_search_query_syntax_tree::language(q).as_deref(),
    )?;
    let Some(bytes) = super::read_parse_source(&p.canonical)? else {
        return Ok(super::source_limit(&p.canonical.to_string_lossy(), None));
    };
    let sanitized = security
        .validate_text_bytes(&bytes, Some(&p.canonical), MAX_SOURCE)
        .map_err(super::policy_error)?;
    let snapshot = crate::digest::json_sha256(&json!([
        // Node-row shape version (1-based columns).
        "rows-v2",
        p.canonical.to_string_lossy(),
        q.language,
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
        extension: super::parser_override(
            &p.canonical,
            ast_search_query_syntax_tree::language(q).as_deref(),
        ),
    };
    let budget = crate::response::page_windows::page_budget(
        response_window,
        &crate::response::result::query_value(q),
    );
    let page = read_page(q, &source, budget, &snapshot, cancel)?;
    Ok(render_tree_page(
        q,
        &p.canonical.to_string_lossy(),
        page,
        &snapshot,
    ))
}

/// A `line` past the file's last line is invalid input naming the line
/// count, repaired by the same tree without `line` (the whole file).
fn check_line(q: &AstSearchQuerySyntaxTree, line: u32, content: &str) -> Result<(), ToolError> {
    let total = content.lines().count();
    if usize_of(line) <= total.max(1) {
        return Ok(());
    }
    let mut error = ToolError::new(
        "invalidInput",
        format!("line {line} is past the end of the file ({total} lines)."),
    );
    let repaired = crate::response::result::query_row(q, &["line", "page", "snapshot"]);
    error.next = Some(Box::new(json!({
        "repair": crate::response::result::Continuation::new(ToolId::AstSearch, repaired).confidence(Confidence::Exact).build()
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

/// Page `q.page()` of the tree. An explicit size asks the engine for
/// exactly its page. Otherwise the page is the nodes that fill the window:
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
    if let Some((offset, size)) =
        ast_search_query_syntax_tree::node_offset(q).zip(ast_search_query_syntax_tree::page_size(q))
    {
        let r = source.inspect(q, offset, size)?;
        let more = r.next_offset.is_some();
        let total = usize_of(r.total_nodes);
        let past_end = page > 1 && r.status == "ok" && usize_of(offset) >= total;
        let facts = PageFacts::open(page, Some(usize_of(size)), more)
            .with_total(total)
            .out_of_range(past_end);
        return Ok(TreePage { r, more, facts });
    }
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

/// The row of one read page: its nodes and diagnostics, pagination and the
/// next page; a failed inspection is an error row.
fn render_tree_page(
    q: &AstSearchQuerySyntaxTree,
    display_path: &str,
    TreePage { r, more, facts }: TreePage,
    snapshot: &str,
) -> serde_json::Value {
    // Line/column locate every node; byte offsets are verbose (core field
    // class): the verbose stage keeps `returnedBytes` for `debug`.
    let node_bytes = r
        .nodes
        .iter()
        .map(|n| format!("{}-{}", n.start_byte, n.end_byte))
        .collect::<Vec<_>>();
    let nodes = r.nodes.iter().map(node_page_row).collect::<Vec<_>>();
    let codes = r
        .diagnostics
        .iter()
        .map(|d| d.code.clone())
        .collect::<Vec<_>>();
    // A walk's notes do not change between its pages: page 1 states them, and
    // a walk cut is stated where the listing stops (an error row keeps all).
    let first_page = ast_search_query_syntax_tree::page(q) == 1;
    let diagnostics = r
        .diagnostics
        .into_iter()
        .filter(|d| {
            r.status == "error"
                || if is_walk_cut(&d.code) {
                    !more
                } else {
                    first_page
                }
        })
        .map(|d| crate::tools::ast_rule::diagnostic_row(&d, None))
        .collect::<Vec<_>>();
    // A failed inspection is an error row that states its error once: the
    // declared code and message of the engine's first diagnostic. It has no
    // page, snapshot or diagnostics to add.
    if r.status == "error" {
        let first = diagnostics.first();
        let code = first
            .and_then(|diagnostic| diagnostic["code"].as_str())
            .map_or("executionFailed", super::engine_error_code);
        let message = first
            .and_then(|diagnostic| diagnostic["message"].as_str())
            .unwrap_or("The syntax tree could not be built.");
        return json!({"operation":"syntaxTree","path":display_path,"status":"error","errorCode":code,"error":message});
    }
    let page = ast_search_query_syntax_tree::page(q);
    let complete = r.status == "ok" && !more;
    let pagination = facts.to_value();
    let mut out = json!({"operation":"syntaxTree","path":display_path,"nodes":nodes,"returnedBytes":node_bytes,"snapshot":snapshot,"isPartial":!complete,
        "pagination":pagination});
    if !diagnostics.is_empty() {
        out["diagnostics"] = json!(diagnostics);
    }
    if more {
        let next_page = crate::response::paging::next_cursor(
            &mut out,
            ToolId::AstSearch,
            Some("syntaxTree"),
            "page",
            usize_of(page) + 1,
        );
        if let Some(next_page) = next_page {
            let mut nq = crate::response::result::query_row(q, &[]);
            nq["snapshot"] = json!(snapshot);
            nq["page"] = json!(next_page);
            out["next"] = json!({"nextPage":crate::response::result::Continuation::new(ToolId::AstSearch, nq).confidence(Confidence::Exact).build()})
        }
    } else if r.status == "partial" {
        for reason in walk_cuts(&codes) {
            crate::response::paging::terminal_limit(&mut out, reason, &[]);
        }
    }
    out
}

/// A walk-budget stop: the listing ends there, not at the tree's end.
fn is_walk_cut(code: &str) -> bool {
    code.starts_with("syntaxTree.budget.")
}

/// The `partialReasons` of a partial walk, from its engine diagnostics: a
/// recovered parse, the walk deadline, the node limit.
fn walk_cuts(codes: &[String]) -> Vec<&'static str> {
    let reasons = codes
        .iter()
        .filter_map(|code| match code.as_str() {
            "syntaxTree.parse.recovered" => Some("parseRecovered"),
            "syntaxTree.budget.deadline" => Some("deadline"),
            "syntaxTree.budget.nodeLimit" => Some("nodeLimit"),
            _ => None,
        })
        .collect::<Vec<_>>();
    if reasons.is_empty() {
        vec!["walkCut"]
    } else {
        reasons
    }
}
/// Nodes read from the engine per call while a window-sized page is cut.
const NODE_CHUNK: u32 = 100_000;

fn node_page_row(n: &octocode_engine::structural::SyntaxTreeNode) -> String {
    node_row(
        n.id,
        &n.kind,
        n.named,
        [n.start_line, n.start_column, n.end_line, n.end_column],
        n.parent_id,
    )
}

/// Characters a node adds to its delivered page: its row. Its
/// `returnedBytes` entry is a `stats` extra a default row drops, so a page
/// asking for stats may run past the window by those entries.
fn node_page_chars(n: &octocode_engine::structural::SyntaxTreeNode) -> usize {
    crate::response::stream_page::json_text_chars(&node_page_row(n)) + 1
}

/// One node as `"<id> <kind> <startLine>:<startColumn>-<endLine>:<endColumn>
/// ^<parentId>"` (1-based lines and columns, end exclusive). An anonymous token's kind
/// is quoted (`"("`), as tree-sitter prints it; the root has no parent.
fn node_row(id: u32, kind: &str, named: bool, span: [u32; 4], parent: Option<u32>) -> String {
    let [start_line, start_column, end_line, end_column] = span;
    let kind = if named {
        kind.to_owned()
    } else {
        format!("\"{kind}\"")
    };
    // 1-based lines and columns; the end stays exclusive.
    let (start_column, end_column) = (
        crate::num::one_based_column(start_column),
        crate::num::one_based_column(end_column),
    );
    let mut row = format!("{id} {kind} {start_line}:{start_column}-{end_line}:{end_column}");
    if let Some(parent) = parent {
        row.push_str(&format!(" ^{parent}"));
    }
    row
}
