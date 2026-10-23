//! Node identity for one walk: canonical keys, walk order, authorization, and
//! TypeScript built-in lib declarations.

use crate::policy::path::PathPolicy;
use crate::tools::lsp_search::render::uri_to_path;
use crate::tools::lsp_search::source::item_uri_is_authorized;
use serde_json::Value;
use std::collections::HashMap;

/// Whether a hierarchy node is declared in a TypeScript built-in lib file
/// (`…/typescript/lib/lib.<name>.d.ts`): language built-ins, not project or
/// dependency code.
pub(in crate::tools::lsp_search) fn is_builtin_lib_declaration(node: &Value) -> bool {
    let Some(uri) = node.get("uri").and_then(Value::as_str) else {
        return false;
    };
    let Some((dir, file)) = uri.rsplit_once('/') else {
        return false;
    };
    dir.ends_with("/typescript/lib") && file.starts_with("lib.") && file.ends_with(".d.ts")
}

/// A built-in lib item's name, qualified by its container when the server
/// names one (`String.toUpperCase`).
pub(in crate::tools::lsp_search) fn builtin_name(node: &Value) -> String {
    let name = node.get("name").and_then(Value::as_str).unwrap_or("?");
    match node
        .get("detail")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|detail| !detail.is_empty() && !detail.contains(char::is_whitespace))
    {
        Some(container) => format!("{container}.{name}"),
        None => name.to_owned(),
    }
}

/// Memoized node identity and authorization for one walk.
pub(in crate::tools::lsp_search) struct NodeKeys<'a> {
    paths: &'a PathPolicy,
    canonical: HashMap<String, String>,
    authorized: HashMap<String, bool>,
}

impl<'a> NodeKeys<'a> {
    pub(in crate::tools::lsp_search) fn new(paths: &'a PathPolicy) -> Self {
        Self {
            paths,
            canonical: HashMap::new(),
            authorized: HashMap::new(),
        }
    }

    /// `(canonical path, selectionRange start/end)`; `range` when the item
    /// has no selection range.
    pub(in crate::tools::lsp_search) fn key(&mut self, node: &Value) -> String {
        let (path, [start_line, start_character, end_line, end_character]) = self.place(node);
        format!("{path}\u{0}{start_line}:{start_character}-{end_line}:{end_character}")
    }

    /// Walk order of a node: canonical path, anchor range, then name. A
    /// level admits results in this order, so which nodes fit under the
    /// caps never depends on the order the server listed them in.
    pub(super) fn order(&mut self, node: &Value) -> (String, [u64; 4], String) {
        let (path, range) = self.place(node);
        let name = node.get("name").and_then(Value::as_str).unwrap_or_default();
        (path, range, name.to_owned())
    }

    /// The canonical path and anchor range (start line/character, end
    /// line/character) of a node.
    fn place(&mut self, node: &Value) -> (String, [u64; 4]) {
        let uri = node.get("uri").and_then(Value::as_str).unwrap_or_default();
        let path = self
            .canonical
            .entry(uri.to_owned())
            .or_insert_with(|| {
                let decoded = uri_to_path(uri);
                std::fs::canonicalize(&decoded)
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or(decoded)
            })
            .clone();
        let range = anchor_range(node);
        let point = |pointer: &str| {
            range
                .and_then(|range| range.pointer(pointer))
                .and_then(Value::as_u64)
                .unwrap_or(0)
        };
        (
            path,
            [
                point("/start/line"),
                point("/start/character"),
                point("/end/line"),
                point("/end/character"),
            ],
        )
    }

    pub(super) fn authorized(&mut self, result: &Value) -> bool {
        let uri = result
            .pointer("/from/uri")
            .or_else(|| result.pointer("/to/uri"))
            .or_else(|| result.get("uri"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if let Some(known) = self.authorized.get(&uri) {
            return *known;
        }
        let allowed = item_uri_is_authorized(result, self.paths);
        self.authorized.insert(uri, allowed);
        allowed
    }
}

/// The LSP name anchor of a hierarchy node: its `selectionRange`, else its
/// whole `range`.
pub(super) fn anchor_range(node: &Value) -> Option<&Value> {
    node.get("selectionRange").or_else(|| node.get("range"))
}
