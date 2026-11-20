//! A read syntax-tree page as its row: one compact line per node, the
//! walk's diagnostics and partial reasons, pagination and the next page.
use super::{TreePage, ast_search_query_syntax_tree};
use octocode_contract_types::{AstSearchQuerySyntaxTree, ErrorCode};
use octocode_engine::num::saturating_usize;
use octocode_runtime_base::id::ToolId;
use serde_json::json;

/// The row of one read page: its nodes and diagnostics, pagination and the
/// next page; a failed inspection is an error row.
pub(super) fn render_tree_page(
    q: &AstSearchQuerySyntaxTree,
    display_path: &str,
    TreePage { r, more, facts }: &TreePage,
    snapshot: &str,
) -> serde_json::Value {
    let provenance = ast_search_query_syntax_tree::provenance(q);
    let nodes = r
        .nodes
        .iter()
        .map(|node| {
            if provenance {
                node_page_row(node)
            } else {
                plain_node_row(node)
            }
        })
        .collect::<Vec<_>>();
    // A walk's notes do not change between its pages: page 1 states them, and
    // a walk cut is stated where the listing stops (an error row keeps all).
    let first_page = ast_search_query_syntax_tree::page(q) == 1;
    let stated = |code: &str| {
        if is_walk_cut(code) {
            !*more
        } else {
            first_page
        }
    };
    // A diagnostic the row states as a partial reason is that reason alone,
    // unless its recovery adds the repair.
    let diagnostics = r
        .diagnostics
        .iter()
        .map(|d| octocode_tools_shared::ast_rule::diagnostic_row(d, None))
        .filter(|row| {
            let code = row["code"].as_str().unwrap_or_default();
            r.status == "error"
                || (stated(code)
                    && (partial_reason(code).is_none() || row.get("recovery").is_some()))
        })
        .collect::<Vec<_>>();
    // A failed inspection is an error row that states its error once: the
    // declared code and message of the engine's first diagnostic. It has no
    // page, snapshot or diagnostics to add.
    if r.status == "error" {
        let first = diagnostics.first();
        let code = first
            .and_then(|diagnostic| diagnostic["code"].as_str())
            .map_or(
                ErrorCode::ExecutionFailed,
                super::super::errors::engine_error_code,
            );
        let message = first
            .and_then(|diagnostic| diagnostic["message"].as_str())
            .unwrap_or("The syntax tree could not be built.");
        let mut out = json!({"operation":"syntaxTree","path":display_path,"status":"error","errorCode":code,"error":message});
        if code == ErrorCode::LanguageUnsupported
            && let Some(read) = super::super::output::unsupported_read(q.path.as_str())
        {
            octocode_runtime_base::response::result::attach_related(&mut out, read);
        }
        return out;
    }
    let page = ast_search_query_syntax_tree::page(q);
    let pagination = facts.to_value();
    // The snapshot pins the next page through its cursor only.
    let mut out =
        json!({"operation":"syntaxTree","path":display_path,"nodes":nodes,"pagination":pagination});
    // More pages are pagination (`hasMore` and the next page), never
    // partial coverage: only a partial walk is partial.
    octocode_runtime_base::response::paging::set_partial(&mut out, r.status != "ok");
    // A tree over one page names its size and how to read a part of it
    // before every page is read.
    if first_page && *more {
        octocode_runtime_base::response::paging::push_notice(
            &mut out,
            narrowing_notice(r.total_nodes, r.nodes.len()),
        );
    }
    if !diagnostics.is_empty() {
        out["diagnostics"] = json!(diagnostics);
    }
    if *more
        && let Some(lead) = octocode_runtime_base::response::collection::continued_page(
            ToolId::AstSearch,
            octocode_runtime_base::response::result::query_row(q, &[]),
            "page",
            saturating_usize(page) + 1,
            &[("snapshot", json!(snapshot))],
        )
    {
        out["calls"] =
            octocode_runtime_base::response::collection::walk_calls(ToolId::AstSearch, None, lead);
    }
    if r.status == "partial" {
        use octocode_runtime_base::response::paging::{mark_partial, terminal_limit};
        for (code, reason) in r
            .diagnostics
            .iter()
            .filter_map(|d| Some((d.code.as_str(), partial_reason(&d.code)?)))
        {
            // A cut ends the listing where no page continues it; before
            // there, and for any other reason, every page names the reason,
            // so a page read alone says why it is partial.
            if is_walk_cut(code) && !*more {
                terminal_limit(&mut out, reason, &[]);
            } else {
                mark_partial(&mut out, reason, &[]);
            }
        }
    }
    out
}

/// How a tree of `total` nodes, `shown` on its first page, is read in part.
fn narrowing_notice(total: u32, shown: usize) -> String {
    format!(
        "The tree holds {total} nodes, {shown} on this page; to read fewer, set line to a line inside the declaration you need: the tree then starts at the smallest node covering it."
    )
}

/// A walk-budget stop: the listing ends there, not at the tree's end.
fn is_walk_cut(code: &str) -> bool {
    code.starts_with("syntaxTree.budget.")
}

/// The partial reason an engine diagnostic of a partial walk states: a
/// recovered parse or the walk deadline, the only two causes the engine
/// marks a walk partial for.
fn partial_reason(code: &str) -> Option<octocode_runtime_base::response::paging::PartialReason> {
    use octocode_runtime_base::response::paging::PartialReason;
    match code {
        "syntaxTree.parse.recovered" => Some(PartialReason::ParseRecovered),
        "syntaxTree.budget.deadline" => Some(PartialReason::Deadline),
        _ => None,
    }
}

/// One node's line alone: what a row is unless provenance was asked for.
pub(super) fn plain_node_row(n: &octocode_engine::structural::SyntaxTreeNode) -> serde_json::Value {
    json!({"node": node_line(n)})
}

fn node_line(n: &octocode_engine::structural::SyntaxTreeNode) -> String {
    node_row(
        n.id,
        (n.field.as_deref(), &n.kind),
        n.named,
        [n.start_line, n.start_column, n.end_line, n.end_column],
        n.parent_id,
    )
}

pub(super) fn node_page_row(n: &octocode_engine::structural::SyntaxTreeNode) -> serde_json::Value {
    let node = node_line(n);
    json!({"node": node, "sourceRange": {"startByte": n.source_span.start_byte, "endByte": n.source_span.end_byte}})
}

/// One node as `"<id> [<field>:]<kind> <startLine>:<startColumn>-<endLine>:<endColumn>
/// ^<parentId>"` (1-based lines and columns, end exclusive). The grammar field
/// the node fills in its parent prefixes its kind (`name:identifier`), as
/// tree-sitter prints `name: (identifier)`; an anonymous token's kind is
/// quoted (`"("`); the root has no parent.
fn node_row(
    id: u32,
    (field, kind): (Option<&str>, &str),
    named: bool,
    span: [u32; 4],
    parent: Option<u32>,
) -> String {
    let [start_line, start_column, end_line, end_column] = span;
    let kind = if named {
        kind.to_owned()
    } else {
        format!("\"{kind}\"")
    };
    let kind = match field {
        Some(field) => format!("{field}:{kind}"),
        None => kind,
    };
    // 1-based lines and columns; the end stays exclusive.
    let (start_column, end_column) = (
        octocode_runtime_base::num::one_based_column(start_column),
        octocode_runtime_base::num::one_based_column(end_column),
    );
    let mut row = format!("{id} {kind} {start_line}:{start_column}-{end_line}:{end_column}");
    if let Some(parent) = parent {
        row.push_str(&format!(" ^{parent}"));
    }
    row
}
