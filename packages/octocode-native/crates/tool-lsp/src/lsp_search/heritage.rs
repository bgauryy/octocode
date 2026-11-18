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
use octocode_engine::lsp::resolver::canonical_symbol_name;
use octocode_engine::lsp::types::{LspCodeSnippet, LspExactPosition, LspRange};
use octocode_engine::signatures::TYPE_DECLARATION_KINDS;
use std::collections::BTreeSet;
use std::future::Future;

mod scan;
#[cfg(test)]
pub(super) use scan::member_snippets;
use scan::scan_members;
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

/// Subtypes read from the heritage clauses of every workspace file that
/// spells the queried name: the syntax answer standing by for a failed
/// implementation request, or beside the answer of a rung that gave one.
/// `None` when no name is queried or the text scan of the workspace could
/// not run.
async fn scan_workspace(
    query: &super::LspSearchQuery,
    scope: &super::scope::Scope,
    policy: &octocode_runtime_base::policy::path::PathPolicy,
    cancel: &dyn octocode_runtime_base::execution::cancel::CancellationCheck,
) -> Result<Option<HeritageScan>, LspFailure> {
    let Some(base) =
        super::lsp_search_query::symbol_name(query).filter(|name| !name.trim().is_empty())
    else {
        return Ok(None);
    };
    let language = super::lsp_search_query::path(query)
        .and_then(octocode_engine::lsp::config::detect_language_id);
    let wanted = canonical_symbol_name(language.as_deref(), base);
    let Some(files) = scope.text_files(wanted, policy, cancel).await? else {
        return Ok(None);
    };
    let files = std::sync::Arc::new(
        reaching(query, scope, (wanted, wanted), &files, policy, cancel).await?,
    );
    let name = wanted.to_owned();
    let scanned = files.clone();
    let Some((found, unread)) = super::blocking_cancellable(cancel, move |stopped| {
        scan_subtypes(&scanned, &name, stopped)
    })
    .await?
    else {
        return Ok(None);
    };
    Ok(Some(HeritageScan {
        base: base.to_owned(),
        files: files.len(),
        found,
        unread,
    }))
}

/// The scanned `files` that may hold a subtype of the anchor's type `base`
/// (or a member of it, scanned by `scanned`): the anchor's own file, and a
/// file in the reach of the anchor's declaration that names that type. In
/// TS/JS a file names the anchor's exported type only through a module
/// reference that may reach the anchor's module; in other languages a file
/// that declares its own type of that name names its own.
async fn reaching(
    query: &super::LspSearchQuery,
    scope: &super::scope::Scope,
    (base, scanned): (&str, &str),
    files: &BTreeSet<String>,
    policy: &octocode_runtime_base::policy::path::PathPolicy,
    cancel: &dyn octocode_runtime_base::execution::cancel::CancellationCheck,
) -> Result<BTreeSet<String>, LspFailure> {
    let (Some(anchor), Some(symbol)) = (
        super::lsp_search_query::path(query),
        super::lsp_search_query::symbol_name(query),
    ) else {
        return Ok(files.clone());
    };
    let anchor = super::scope::canonical(anchor);
    let in_reach = files
        .iter()
        .filter(|file| **file == anchor || scope.may_reach(symbol, file))
        .cloned()
        .collect::<BTreeSet<_>>();
    let language = octocode_engine::lsp::config::detect_language_id(&anchor);
    if octocode_engine::lsp::family::is_typescript_family(language.as_deref()) {
        return super::importers::reachable_files(
            scope,
            (&anchor, base, scanned),
            &in_reach,
            policy,
            cancel,
        )
        .await;
    }
    let (base, policy) = (base.to_owned(), policy.clone());
    let kept = super::blocking_cancellable(cancel, move |stopped| {
        in_reach
            .into_iter()
            .filter(|file| stopped() || *file == anchor || !declares_own_type(&policy, file, &base))
            .collect::<BTreeSet<_>>()
    })
    .await?;
    Ok(kept.unwrap_or_else(|| files.clone()))
}

/// Whether `file` declares a type named `base` at its top level (a file that
/// cannot be read declares none that is known).
fn declares_own_type(
    policy: &octocode_runtime_base::policy::path::PathPolicy,
    file: &str,
    base: &str,
) -> bool {
    let Ok(valid) = policy.validate_read(file) else {
        return false;
    };
    let Ok(content) = read_bounded_source(&valid.canonical) else {
        return false;
    };
    octocode_tools_shared::declarations::file_document(&content, file).is_some_and(|document| {
        document.declarations.iter().any(|declaration| {
            declaration.parent.is_none()
                && declaration.name == base
                && TYPE_DECLARATION_KINDS.contains(&declaration.kind.as_str())
        })
    })
}

/// What a heritage scan can read for an `implementation` anchor.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Scannable {
    /// A type declaration: the declarations whose heritage clause names it.
    Subtypes,
    /// A member of the type `owner` (a trait or interface method): the
    /// members of that name the type's subtypes declare.
    Members { owner: String },
}

impl Scannable {
    /// Whether the scan can stand in for a server that did not answer: a
    /// type's subtypes are its implementations, while a member's rows
    /// only add to an answer the server gave.
    pub(super) fn stands_in(&self) -> bool {
        *self == Self::Subtypes
    }
}

/// What a heritage scan can read for the anchor `name` at the zero-based
/// `line` of `content`; `None` for an anchor no heritage clause reaches (a
/// function, a variable, a member of an impl block).
pub(super) fn scannable(content: &str, file: &str, line: u32, name: &str) -> Option<Scannable> {
    if scan::anchors_type_declaration(content, file, line, name) {
        return Some(Scannable::Subtypes);
    }
    scan::member_owner(content, file, line, name).map(|owner| Scannable::Members { owner })
}

/// The scan `what` names for the query's symbol ([`scan_workspace`] for a
/// type, the members its subtypes declare for a member).
pub(super) async fn scan_for(
    what: &Scannable,
    query: &super::LspSearchQuery,
    scope: &super::scope::Scope,
    policy: &octocode_runtime_base::policy::path::PathPolicy,
    cancel: &dyn octocode_runtime_base::execution::cancel::CancellationCheck,
) -> Result<Option<HeritageScan>, LspFailure> {
    let owner = match what {
        Scannable::Subtypes => return scan_workspace(query, scope, policy, cancel).await,
        Scannable::Members { owner } => owner,
    };
    let Some(member) =
        super::lsp_search_query::symbol_name(query).filter(|name| !name.trim().is_empty())
    else {
        return Ok(None);
    };
    // A subtype that declares the member spells its name: the files that
    // spell it hold every such declaration.
    let language = super::lsp_search_query::path(query)
        .and_then(octocode_engine::lsp::config::detect_language_id);
    let member = canonical_symbol_name(language.as_deref(), member);
    let Some(files) = scope.text_files(member, policy, cancel).await? else {
        return Ok(None);
    };
    let files =
        std::sync::Arc::new(reaching(query, scope, (owner, member), &files, policy, cancel).await?);
    let (base, name, scanned) = (owner.clone(), member.to_owned(), files.clone());
    let Some((found, unread)) = super::blocking_cancellable(cancel, move |stopped| {
        scan_members(&scanned, &base, &name, stopped)
    })
    .await?
    else {
        return Ok(None);
    };
    Ok(Some(HeritageScan {
        base: owner.clone(),
        files: files.len(),
        found,
        unread,
    }))
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

/// Whether the traits of the type `node` (in `path`) are named apart from
/// its declaration: a Rust struct or enum names none itself, and each
/// `impl Trait for Type` names one ([`impl_node`]).
pub(super) fn named_by_impls(node: &serde_json::Value, path: &str) -> bool {
    matches!(
        node.get("kind").and_then(serde_json::Value::as_str),
        Some("struct" | "enum")
    ) && octocode_engine::lsp::config::detect_language_id(path).as_deref() == Some("rust")
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
                let column = u32::try_from(text[..offset].encode_utf16().count())
                    .map_err(|error| error.to_string())?;
                if column >= start
                    && column < end
                    && octocode_engine::text::is_whole_word(text.as_bytes(), offset, suffix)
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

pub(super) fn type_document(
    content: &str,
    path: &str,
) -> Result<std::sync::Arc<GraphFactsDocument>, String> {
    let document = octocode_tools_shared::declarations::graph_facts(content, path)
        .ok_or_else(|| format!("Type heritage parsing unavailable for {path}"))?;
    // Every tree-sitter document carries the static syntax-only caveat, and
    // a linking caveat (an unexpanded Rust macro's imports, module
    // attributes) leaves every parsed declaration and its own heritage
    // clause whole: neither marks a gap. Any other diagnostic (recovered
    // parse, deadline) means declarations or heritage clauses may be missing.
    let gaps = document
        .diagnostics
        .iter()
        .filter(|note| note.code != GraphFactCode::SyntaxOnly && !note.code.is_linking_only())
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
