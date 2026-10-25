//! A read syntax-tree page as its row: one compact line per node, the
//! walk's diagnostics and partial reasons, pagination and the next page.
use super::{TreePage, ast_search_query_syntax_tree};
use crate::id::ToolId;
use crate::num::usize_of;
use octocode_contract_types::{AstSearchQuerySyntaxTree, ErrorCode};
use serde_json::json;

/// The row of one read page: its nodes and diagnostics, pagination and the
/// next page; a failed inspection is an error row.
pub(super) fn render_tree_page(
    q: &AstSearchQuerySyntaxTree,
    display_path: &str,
    TreePage { r, more, facts }: TreePage,
    snapshot: &str,
) -> serde_json::Value {
    let byte_ranges = r.nodes.iter().map(node_byte_range).collect::<Vec<_>>();
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
            .map_or(
                ErrorCode::ExecutionFailed,
                super::super::errors::engine_error_code,
            );
        let message = first
            .and_then(|diagnostic| diagnostic["message"].as_str())
            .unwrap_or("The syntax tree could not be built.");
        return json!({"operation":"syntaxTree","path":display_path,"status":"error","errorCode":code,"error":message});
    }
    let page = ast_search_query_syntax_tree::page(q);
    let complete = r.status == "ok" && !more;
    let pagination = facts.to_value();
    let mut out = json!({"operation":"syntaxTree","path":display_path,"nodes":nodes,"byteRanges":byte_ranges,"snapshot":snapshot,"isPartial":!complete,
        "pagination":pagination});
    if !diagnostics.is_empty() {
        out["diagnostics"] = json!(diagnostics);
    }
    if more {
        if let Some(lead) = crate::response::paging::next_page_call(
            &mut out,
            ToolId::AstSearch,
            Some("syntaxTree"),
            crate::response::result::query_row(q, &[]),
            "page",
            usize_of(page) + 1,
            &[("snapshot", json!(snapshot))],
        ) {
            out["calls"] = json!({ "nextPage": lead });
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

fn node_page_row(n: &octocode_engine::structural::SyntaxTreeNode) -> String {
    node_row(
        n.id,
        (n.field.as_deref(), &n.kind),
        n.named,
        [n.start_line, n.start_column, n.end_line, n.end_column],
        n.parent_id,
    )
}

fn node_byte_range(n: &octocode_engine::structural::SyntaxTreeNode) -> String {
    format!("{}-{}", n.start_byte, n.end_byte)
}

/// Characters a node adds to both arrays, including quotes and separators.
pub(super) fn node_page_chars(n: &octocode_engine::structural::SyntaxTreeNode) -> usize {
    octocode_engine::json_size::string_body_units(&node_page_row(n)) + node_byte_range(n).len() + 6
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
        crate::num::one_based_column(start_column),
        crate::num::one_based_column(end_column),
    );
    let mut row = format!("{id} {kind} {start_line}:{start_column}-{end_line}:{end_column}");
    if let Some(parent) = parent {
        row.push_str(&format!(" ^{parent}"));
    }
    row
}
