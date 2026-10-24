//! Type hierarchy without `typeHierarchyProvider`.
//!
//! The shared walker follows source heritage clauses resolved by the server.
//! Implementation locations are candidate subtypes: their resolved immediate
//! heritage establishes whether they are direct children of the queried type
//! (a Rust trait impl is the candidate itself, see [`impl_node`]). When
//! implementation requests fail, the separate source scan remains an
//! explicitly unverified, partial syntax answer; beside a server answer to
//! `implementation`, its declarations the server left out are listed the
//! same way.

use super::failure::LspFailure;
use super::source::read_bounded_source;
use octocode_contract_types::ErrorCode;
use octocode_engine::graph::{GraphFactCode, GraphFactsDocument};
use octocode_engine::lsp::types::{LspCodeSnippet, LspExactPosition, LspRange};
use octocode_engine::signatures::TYPE_DECLARATION_KINDS;
use std::collections::BTreeSet;
use std::future::Future;

mod scan;
#[cfg(test)]
pub(super) use scan::subtype_snippets;
pub(super) use scan::{
    HeritageScan, SERVER_GRACE_AFTER_SCAN, describe_scan, describe_unread, hedged, scan_subtypes,
};

/// The LSP capability each fallback needs.
pub(super) fn fallback_provider(operation: &str) -> Option<&'static str> {
    match operation {
        "subtypes" => Some("implementationProvider"),
        "supertypes" => Some("definitionProvider"),
        _ => None,
    }
}

/// One base type named by a declaration's heritage clause.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct BaseType {
    /// The base type as written (`Base`, `ns.Base`).
    pub(super) name: String,
    /// `extends` or `implements`.
    pub(super) relation: String,
    /// Zero-based position of the name's last segment in the source.
    pub(super) line: u32,
    pub(super) character: u32,
}

pub(super) fn type_node(
    document: &GraphFactsDocument,
    path: &str,
    line: u32,
    character: u32,
) -> Result<serde_json::Value, String> {
    let point = octocode_engine::graph::GraphPosition { line, character };
    let declaration = document
        .declarations
        .iter()
        .find(|declaration| {
            TYPE_DECLARATION_KINDS.contains(&declaration.kind.as_str())
                && ((declaration.selection_range.start <= point
                    && point < declaration.selection_range.end)
                    || declaration.range.start == point)
        })
        .ok_or_else(|| {
            format!(
                "No parsed type declaration at {path}:{}:{}",
                line + 1,
                character + 1
            )
        })?;
    Ok(serde_json::json!({
        "uri": octocode_engine::lsp::uri::path_to_uri(path).map_err(|error| error.to_string())?,
        "name": declaration.name, "kind": declaration.kind,
        "range": declaration.range, "selectionRange": declaration.selection_range
    }))
}

/// The trait implementation (`impl Trait for Type`) whose header holds
/// `line:character`, where a server locates it (rust-analyzer: at the self
/// type), as a hierarchy node named by its self type. `None` for any other
/// declaration: an inherent `impl Type` implements nothing.
pub(super) fn impl_node(
    document: &GraphFactsDocument,
    path: &str,
    line: u32,
    character: u32,
) -> Option<serde_json::Value> {
    let point = octocode_engine::graph::GraphPosition { line, character };
    let declaration = document
        .declarations
        .iter()
        .filter(|declaration| {
            declaration.kind == "impl"
                && declaration.range.start <= point
                && point < declaration.selection_range.end
                && document
                    .edges
                    .iter()
                    .any(|edge| edge.from == declaration.id && edge.relation == "implements")
        })
        // The innermost: an impl nested in a function body of another.
        .max_by(|left, right| left.range.start.cmp(&right.range.start))?;
    Some(serde_json::json!({
        "uri": octocode_engine::lsp::uri::path_to_uri(path).ok()?,
        "name": declaration.name, "kind": declaration.kind,
        "range": declaration.range, "selectionRange": declaration.selection_range
    }))
}

pub(super) fn direct_bases(
    document: &GraphFactsDocument,
    content: &str,
    path: &str,
    node: &serde_json::Value,
) -> Result<Vec<BaseType>, String> {
    let declaration = document
        .declarations
        .iter()
        .find(|declaration| {
            serde_json::to_value(&declaration.selection_range)
                .ok()
                .as_ref()
                == node.get("selectionRange")
        })
        .ok_or_else(|| format!("The type declaration changed in {path}"))?;
    let lines = content.lines().collect::<Vec<_>>();
    document
        .edges
        .iter()
        .filter(|edge| {
            edge.from == declaration.id
                && matches!(edge.relation.as_str(), "extends" | "implements")
        })
        .map(|edge| {
            let line = edge
                .line
                .checked_sub(1)
                .ok_or_else(|| "Invalid heritage line".to_owned())?;
            let text = lines
                .get(usize::try_from(line).map_err(|error| error.to_string())?)
                .ok_or_else(|| "Heritage line unavailable".to_owned())?;
            // A trait implementation names its trait before its self type,
            // the declaration's name (`impl<T: Bound> Trait for Type`): the
            // clause is the last spelling before the name. Elsewhere the
            // clause follows the name and spells the base once.
            let clause_first = declaration.kind == "impl";
            let (start, end) = if clause_first {
                (
                    if line == declaration.range.start.line {
                        declaration.range.start.character
                    } else {
                        0
                    },
                    if line == declaration.selection_range.start.line {
                        declaration.selection_range.start.character
                    } else {
                        u32::MAX
                    },
                )
            } else {
                (
                    if line == declaration.selection_range.end.line {
                        declaration.selection_range.end.character
                    } else {
                        0
                    },
                    if line == declaration.range.end.line {
                        declaration.range.end.character
                    } else {
                        u32::MAX
                    },
                )
            };
            let leaf = edge
                .to
                .rsplit(['.', ':'])
                .next()
                .filter(|leaf| !leaf.is_empty())
                .ok_or_else(|| "Heritage name unavailable".to_owned())?;
            let mut from = 0;
            let mut positions = Vec::new();
            while let Some(found) = text[from..].find(&edge.to) {
                let offset = from + found;
                let suffix = offset + edge.to.len();
                let is_word = |ch: char| ch.is_alphanumeric() || ch == '_' || ch == '$';
                let column = u32::try_from(text[..offset].encode_utf16().count())
                    .map_err(|error| error.to_string())?;
                if column >= start
                    && column < end
                    && !text[..offset].chars().next_back().is_some_and(is_word)
                    && !text[suffix..].chars().next().is_some_and(is_word)
                {
                    let prefix = &edge.to[..edge.to.len() - leaf.len()];
                    positions.push(
                        column
                            + u32::try_from(prefix.encode_utf16().count())
                                .map_err(|error| error.to_string())?,
                    );
                }
                from = suffix;
            }
            let anchor = match positions.as_slice() {
                [.., last] if clause_first => Some(last),
                [only] => Some(only),
                _ => None,
            };
            let Some(character) = anchor else {
                return Err(format!(
                    "Cannot establish an exact heritage anchor for {} in {path}:{}",
                    edge.to,
                    line + 1
                ));
            };
            Ok(BaseType {
                name: edge.to.clone(),
                relation: edge.relation.clone(),
                line,
                character: *character,
            })
        })
        .collect()
}

pub(super) fn type_document(content: &str, path: &str) -> Result<GraphFactsDocument, String> {
    let facts = octocode_engine::signatures::extract_graph_facts(content, path)
        .ok_or_else(|| format!("Type heritage parsing unavailable for {path}"))?;
    let document: GraphFactsDocument = serde_json::from_str(&facts)
        .map_err(|error| format!("Type heritage parsing failed for {path}: {error}"))?;
    // Every tree-sitter document carries the static syntax-only caveat; it
    // marks no gap. Any other diagnostic (recovered parse, deadline, Rust
    // attributes) means declarations or heritage clauses may be missing.
    let gaps = document
        .diagnostics
        .iter()
        .filter(|note| note.code != GraphFactCode::SyntaxOnly)
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if !document.error_lines.is_empty()
        || !gaps.is_empty()
        || document.rust_root_unsupported == Some(true)
    {
        return Err(format!(
            "Type heritage parse is incomplete for {path}: {}",
            gaps.join("; ")
        ));
    }
    Ok(document)
}

/// Whether the zero-based `line` holds the declaration of the type `name`:
/// the only anchor whose subtypes a heritage scan can list. A member, a
/// function or a variable has no extends/implements clause naming it, so a
/// scan for it would report "no implementations" instead of "unanswered".
pub(super) fn anchors_type_declaration(content: &str, file: &str, line: u32, name: &str) -> bool {
    let Some(facts) = octocode_engine::signatures::extract_graph_facts(content, file) else {
        return false;
    };
    let Ok(document) = serde_json::from_str::<GraphFactsDocument>(&facts) else {
        return false;
    };
    document.declarations.iter().any(|declaration| {
        declaration.name == name
            && declaration.selection_range.start.line == line
            && TYPE_DECLARATION_KINDS.contains(&declaration.kind.as_str())
    })
}

/// Whether a failed `textDocument/implementation` request leaves the syntax
/// scan as a cheaper labelled answer: the server could not answer (timeout,
/// exit, rejected request), as opposed to the caller cancelling or a refused
/// path.
pub(super) fn implementation_failure_has_fallback(failure: &LspFailure) -> bool {
    matches!(
        failure.code,
        ErrorCode::Timeout
            | ErrorCode::ServerCrashed
            | ErrorCode::RequestFailed
            | ErrorCode::CapabilityUnavailable
    )
}

#[cfg(test)]
mod tests;
