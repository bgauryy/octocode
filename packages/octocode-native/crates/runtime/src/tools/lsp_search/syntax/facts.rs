//! Blocking tree-sitter fact scans the syntax rung answers from: the
//! declarations, name occurrences, calls and syntax errors of source files,
//! in the LSP shapes the payload builders take.
use super::super::source::read_bounded_source;
use crate::tools::declarations;
use octocode_engine::graph::{GraphFactDeclaration, GraphFactsDocument, GraphRange};
use octocode_engine::lsp::types::{LspCodeSnippet, LspExactPosition, LspRange};
use octocode_engine::num::saturating_u32;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::Arc;

/// One source file the scan read: its canonical path, URI and text.
pub(super) struct File {
    pub(super) path: String,
    pub(super) uri: String,
    pub(super) content: String,
}

impl File {
    fn read(path: &str) -> Option<Self> {
        Some(Self {
            path: path.to_owned(),
            uri: octocode_engine::lsp::uri::path_to_uri(path).ok()?,
            content: read_bounded_source(std::path::Path::new(path)).ok()?,
        })
    }

    /// The file's declarations; `None` when its type has no grammar.
    pub(super) fn outline(&self) -> Option<Arc<GraphFactsDocument>> {
        declarations::file_document(&self.content, &self.path)
    }

    /// The file's whole graph facts (calls, heritage edges, error lines).
    pub(super) fn facts(&self) -> Option<Arc<GraphFactsDocument>> {
        declarations::graph_facts(&self.content, &self.path)
    }

    /// The location of `range` in this file, read as a server location
    /// reads: the text of its first line.
    pub(super) fn snippet(&self, range: &GraphRange, kind: Option<&str>) -> LspCodeSnippet {
        LspCodeSnippet {
            uri: self.uri.clone(),
            range: lsp_range(range),
            content: self
                .content
                .lines()
                .nth(range.start.line as usize)
                .unwrap_or_default()
                .to_owned(),
            symbol_kind: kind.map(str::to_owned),
            display_range: None,
        }
    }

    /// A hierarchy node (an LSP `CallHierarchyItem`/`TypeHierarchyItem`)
    /// for `declaration` of this file.
    pub(super) fn node(&self, declaration: &GraphFactDeclaration) -> Value {
        json!({
            "name": declaration.name,
            "kind": declaration.kind,
            "uri": self.uri,
            "range": range_value(&declaration.range),
            "selectionRange": range_value(&declaration.selection_range),
        })
    }
}

/// `job` of every readable file of `files`, in path order, on the worker
/// threads, and the files that could not be read; nothing once `stopped`.
pub(super) fn each_file<R: Send>(
    files: &BTreeSet<String>,
    stopped: &(dyn Fn() -> bool + Sync),
    job: impl Fn(&File) -> R + Sync,
) -> (Vec<R>, Vec<String>) {
    let files = files.iter().collect::<Vec<_>>();
    let workers = crate::tools::workers::workers(files.len(), None);
    let scanned = crate::tools::workers::each_parallel(files, workers, |path| {
        if stopped() {
            return None;
        }
        Some(File::read(path).map(|file| job(&file)).ok_or(path))
    });
    let (mut found, mut unread) = (Vec::new(), Vec::new());
    for outcome in scanned.into_iter().flatten() {
        match outcome {
            Ok(result) => found.push(result),
            Err(path) => unread.push(path.clone()),
        }
    }
    (found, unread)
}

/// The declarations named `name` in `files`, at their names.
pub(super) fn declared(
    files: &BTreeSet<String>,
    name: &str,
    stopped: &(dyn Fn() -> bool + Sync),
) -> (Vec<LspCodeSnippet>, Vec<String>) {
    let (found, unread) = each_file(files, stopped, |file| {
        file.outline().map_or_else(Vec::new, |outline| {
            outline
                .declarations
                .iter()
                .filter(|declaration| declaration.name == name)
                .map(|declaration| {
                    file.snippet(&declaration.selection_range, Some(&declaration.kind))
                })
                .collect()
        })
    });
    (found.into_iter().flatten().collect(), unread)
}

/// Every whole-identifier occurrence of `name` in `files`.
pub(super) fn occurrences(
    files: &BTreeSet<String>,
    name: &str,
    stopped: &(dyn Fn() -> bool + Sync),
) -> (Vec<LspCodeSnippet>, Vec<String>) {
    let (found, unread) = each_file(files, stopped, |file| {
        let mut found = Vec::new();
        for (line, text) in file.content.lines().enumerate() {
            for (start, _) in text.match_indices(name) {
                let end = start + name.len();
                let (Some(before), Some(after)) = (text.get(..start), text.get(end..)) else {
                    continue;
                };
                let bounded = !before.chars().next_back().is_some_and(is_word)
                    && !after.chars().next().is_some_and(is_word);
                if !bounded {
                    continue;
                }
                let line = saturating_u32(line);
                let column = |byte: usize| {
                    saturating_u32(text.get(..byte).unwrap_or_default().encode_utf16().count())
                };
                found.push(LspCodeSnippet {
                    uri: file.uri.clone(),
                    range: LspRange {
                        start: LspExactPosition {
                            line,
                            character: column(start),
                        },
                        end: LspExactPosition {
                            line,
                            character: column(end),
                        },
                    },
                    content: text.to_owned(),
                    symbol_kind: None,
                    display_range: None,
                });
            }
        }
        found
    });
    (found.into_iter().flatten().collect(), unread)
}

/// The declarations of `files` whose name contains `query` (case-folded),
/// as LSP `SymbolInformation`.
pub(super) fn symbols(
    files: &BTreeSet<String>,
    query: &str,
    stopped: &(dyn Fn() -> bool + Sync),
) -> (Vec<Value>, Vec<String>) {
    let wanted = query.to_lowercase();
    let (found, unread) = each_file(files, stopped, |file| {
        file.outline().map_or_else(Vec::new, |outline| {
            outline
                .declarations
                .iter()
                .filter(|declaration| declaration.name.to_lowercase().contains(&wanted))
                .map(|declaration| {
                    json!({
                        "name": declaration.name,
                        "kind": declaration.kind,
                        "location": {
                            "uri": file.uri,
                            "range": range_value(&declaration.range),
                        },
                    })
                })
                .collect()
        })
    });
    (found.into_iter().flatten().collect(), unread)
}

/// The outline of `document` as nested LSP `DocumentSymbol`s: a declaration
/// is the child of the innermost declaration whose range holds its own.
pub(super) fn outline(document: &GraphFactsDocument) -> Value {
    let mut declarations = document.declarations.iter().collect::<Vec<_>>();
    declarations.sort_by(|a, b| {
        (a.range.start.line, a.range.start.character)
            .cmp(&(b.range.start.line, b.range.start.character))
            .then(
                (b.range.end.line, b.range.end.character)
                    .cmp(&(a.range.end.line, a.range.end.character)),
            )
    });
    // Open containers, outermost first, each with its children so far.
    let mut open: Vec<(&GraphFactDeclaration, Vec<Value>)> = Vec::new();
    let mut roots = Vec::new();
    let close = |open: &mut Vec<(&GraphFactDeclaration, Vec<Value>)>, roots: &mut Vec<Value>| {
        if let Some((declaration, children)) = open.pop() {
            let symbol = json!({
                "name": declaration.name,
                "kind": declaration.kind,
                "range": range_value(&declaration.range),
                "selectionRange": range_value(&declaration.selection_range),
                "children": children,
            });
            match open.last_mut() {
                Some((_, siblings)) => siblings.push(symbol),
                None => roots.push(symbol),
            }
        }
    };
    for declaration in declarations {
        while open
            .last()
            .is_some_and(|(parent, _)| !contains(&parent.range, &declaration.range))
        {
            close(&mut open, &mut roots);
        }
        open.push((declaration, Vec::new()));
    }
    while !open.is_empty() {
        close(&mut open, &mut roots);
    }
    Value::Array(roots)
}

/// The lines a recovered tree-sitter parse of `content` skipped as errors,
/// as LSP diagnostics.
pub(super) fn syntax_errors(document: &GraphFactsDocument) -> Value {
    document
        .error_lines
        .iter()
        .map(|[start, end]| {
            json!({
                "range": {
                    "start": {"line": start.saturating_sub(1), "character": 0},
                    "end": {"line": end.saturating_sub(1), "character": 0},
                },
                "severity": 1,
                "source": "tree-sitter",
                "message": "Syntax error: tree-sitter could not parse these lines.",
            })
        })
        .collect()
}

fn contains(outer: &GraphRange, inner: &GraphRange) -> bool {
    (outer.start.line, outer.start.character) <= (inner.start.line, inner.start.character)
        && (inner.end.line, inner.end.character) <= (outer.end.line, outer.end.character)
}

fn is_word(character: char) -> bool {
    character.is_alphanumeric() || character == '_' || character == '$'
}

pub(super) fn lsp_range(range: &GraphRange) -> LspRange {
    LspRange {
        start: LspExactPosition {
            line: range.start.line,
            character: range.start.character,
        },
        end: LspExactPosition {
            line: range.end.line,
            character: range.end.character,
        },
    }
}

pub(super) fn range_value(range: &GraphRange) -> Value {
    json!({
        "start": {"line": range.start.line, "character": range.start.character},
        "end": {"line": range.end.line, "character": range.end.character},
    })
}
