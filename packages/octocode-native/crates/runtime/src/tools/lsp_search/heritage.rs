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
use octocode_engine::graph::{GraphFactCode, GraphFactsDocument};
use octocode_engine::lsp::types::{LspCodeSnippet, LspExactPosition, LspRange};
use octocode_engine::signatures::TYPE_DECLARATION_KINDS;
use std::collections::BTreeSet;
use std::future::Future;

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
        "timeout" | "serverCrashed" | "requestFailed" | "capabilityUnavailable"
    )
}

/// What the heritage scan read: the subtypes of `base` among the `files`
/// that spell it, and the `unread` files (canonical paths) it could not read.
pub(super) struct HeritageScan {
    pub(super) base: String,
    pub(super) files: usize,
    pub(super) found: Vec<LspCodeSnippet>,
    pub(super) unread: Vec<String>,
}

/// How long the server's `textDocument/implementation` answer may still
/// arrive once the syntax scan has finished: its rows are resolved, the
/// scan's are matched by name, so a server that is merely slow keeps the
/// win, but one that has not answered by then costs no further wait.
pub(super) const SERVER_GRACE_AFTER_SCAN: std::time::Duration = std::time::Duration::from_secs(5);

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
pub(super) async fn hedged<T, E, H>(
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
pub(super) fn subtype_snippets(content: &str, file: &str, base: &str) -> Vec<LspCodeSnippet> {
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
pub(super) fn scan_subtypes(
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
pub(super) fn describe_unread(unread: &[String]) -> String {
    let named = unread.len().min(super::failure::LISTED_TEXT_FILES);
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
pub(super) fn describe_scan(
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Callers hold canonical absolute paths; a node's URI needs one.
    fn absolute(file: &str) -> String {
        std::env::temp_dir()
            .join(file)
            .to_string_lossy()
            .into_owned()
    }

    fn bases(content: &str, file: &str, line: u32) -> Vec<(String, String, u32, u32)> {
        let file = &absolute(file);
        let document = type_document(content, file).expect("graph facts");
        let declaration = document
            .declarations
            .iter()
            .filter(|declaration| {
                declaration.range.start.line <= line
                    && line <= declaration.range.end.line
                    && TYPE_DECLARATION_KINDS.contains(&declaration.kind.as_str())
            })
            .min_by_key(|declaration| declaration.range.end.line - declaration.range.start.line)
            .expect("type declaration");
        let anchor = &declaration.selection_range.start;
        let node = type_node(&document, file, anchor.line, anchor.character).expect("type node");
        direct_bases(&document, content, file, &node)
            .expect("direct heritage")
            .into_iter()
            .map(|b| (b.name, b.relation, b.line, b.character))
            .collect()
    }

    #[test]
    fn a_typescript_class_names_its_extends_and_implements_clauses_with_positions() {
        let source = "interface Run {}\nclass Base {}\nexport class Child extends Base implements Run {\n  go() {}\n}\n";
        let found = bases(source, "a.ts", 3);
        assert_eq!(
            found,
            vec![
                ("Base".into(), "extends".into(), 2, 27),
                ("Run".into(), "implements".into(), 2, 43),
            ]
        );
    }

    #[test]
    fn the_innermost_declaration_holding_the_line_answers() {
        let source = "class A {}\nclass B extends A {\n  m() { return 1; }\n}\n";
        assert_eq!(bases(source, "a.ts", 2).len(), 1);
        assert!(bases(source, "a.ts", 0).is_empty(), "A has no bases");
    }

    #[test]
    fn a_python_class_reads_dotted_bases_at_their_last_segment() {
        let source = "class Child(pkg.Base, Mixin):\n    pass\n";
        assert_eq!(
            bases(source, "a.py", 0),
            vec![
                ("pkg.Base".into(), "extends".into(), 0, 16),
                ("Mixin".into(), "extends".into(), 0, 22),
            ]
        );
    }

    #[test]
    fn a_recovered_parse_proves_no_heritage_but_the_syntax_only_caveat_does_not_block() {
        let clean = type_document("class Child(Base):\n    pass\n", &absolute("a.py"))
            .expect("a clean tree-sitter parse answers");
        assert!(
            clean
                .diagnostics
                .iter()
                .any(|note| note.code == GraphFactCode::SyntaxOnly)
        );
        let broken = type_document("class Child(Base:\n    pass\n", &absolute("a.py"))
            .expect_err("a recovered parse may miss a clause");
        assert!(
            broken.starts_with("Type heritage parse is incomplete"),
            "{broken}"
        );
        assert!(!broken.contains("syntax-only"), "{broken}");
    }

    /// rust-analyzer locates a trait implementation at its self type: that
    /// impl is the subtype node, and its base is the trait it names before
    /// the self type, past a bound that spells the same trait.
    #[test]
    fn a_rust_trait_impl_is_a_node_whose_base_is_its_trait() {
        let source = "trait Scan {}\nstruct Foo<T>(T);\nimpl<T: Scan> Scan for Foo<T> {}\nimpl<T> Foo<T> {}\n";
        let file = &absolute("lib.rs");
        let document = type_document(source, file).expect("graph facts");
        let node = impl_node(&document, file, 2, 23).expect("trait impl");
        assert_eq!(
            (node["name"].as_str(), node["kind"].as_str()),
            (Some("Foo"), Some("impl"))
        );
        let found = direct_bases(&document, source, file, &node)
            .expect("trait clause")
            .into_iter()
            .map(|b| (b.name, b.relation, b.line, b.character))
            .collect::<Vec<_>>();
        assert_eq!(found, vec![("Scan".into(), "implements".into(), 2, 14)]);
        assert!(impl_node(&document, file, 3, 8).is_none(), "inherent impl");
        assert!(impl_node(&document, file, 1, 7).is_none(), "struct");
    }

    #[test]
    fn a_whole_word_match_skips_longer_identifiers() {
        assert_eq!(
            bases("class XBase extends Base {}", "a.ts", 0),
            vec![("Base".into(), "extends".into(), 0, 20)]
        );
    }

    #[test]
    fn heritage_anchors_keep_same_line_declarations_and_utf16_columns_distinct() {
        let content = "const emoji = \"😀\"; class A extends ns.Base {} class B extends Other {}";
        let file = &absolute("a.ts");
        let document = type_document(content, file).expect("graph facts");
        for (name, base) in [("A", "ns.Base"), ("B", "Other")] {
            let offset =
                content.find(&format!("class {name}")).expect("declaration") + "class ".len();
            let column = u32::try_from(content[..offset].encode_utf16().count()).expect("column");
            let node = type_node(&document, file, 0, column).expect("exact declaration");
            let bases = direct_bases(&document, content, file, &node).expect("direct heritage");
            assert_eq!(bases.len(), 1);
            assert_eq!(bases[0].name, base);
            let leaf = base.rsplit('.').next().expect("leaf");
            let offset = content.find(base).expect("base") + base.len() - leaf.len();
            assert_eq!(
                bases[0].character,
                u32::try_from(content[..offset].encode_utf16().count()).expect("column")
            );
        }
    }

    #[test]
    fn only_subtypes_and_supertypes_have_a_fallback() {
        assert_eq!(
            fallback_provider("subtypes"),
            Some("implementationProvider")
        );
        assert_eq!(fallback_provider("supertypes"), Some("definitionProvider"));
        assert_eq!(fallback_provider("callers"), None);
    }

    /// `(line, start, end, content)` of each declaration a source's heritage
    /// clauses name `base` in, zero-based.
    fn subtypes(content: &str, file: &str, base: &str) -> Vec<(u32, u32, u32, String)> {
        subtype_snippets(content, file, base)
            .into_iter()
            .map(|found| {
                (
                    found.range.start.line,
                    found.range.start.character,
                    found.range.end.character,
                    found.content,
                )
            })
            .collect()
    }

    #[test]
    fn subtypes_are_the_declarations_whose_heritage_names_the_base() {
        let source = "interface Base {}\nclass A extends Base {}\nclass B implements Other, Base {}\ninterface C extends ns.Base {}\nclass D extends Other {}\nclass Base2 extends Base {}\n";
        assert_eq!(
            subtypes(source, "/work/a.ts", "Base"),
            vec![
                (1, 6, 7, "A".into()),
                (2, 6, 7, "B".into()),
                (3, 10, 11, "C".into()),
                (5, 6, 11, "Base2".into()),
            ]
        );
        assert!(subtypes(source, "/work/a.ts", "Missing").is_empty());
    }

    #[test]
    fn only_a_type_declaration_anchors_a_subtype_scan() {
        let source = "export interface IDisposable {\n  dispose(): void;\n}\nexport class Box {\n  run(): void {}\n}\nexport function make(): void {}\n";
        let on = |line: u32, name: &str| anchors_type_declaration(source, "/work/a.ts", line, name);
        assert!(on(0, "IDisposable"), "an interface");
        assert!(on(3, "Box"), "a class");
        assert!(
            !on(1, "dispose"),
            "an interface member has no subtypes to scan"
        );
        assert!(
            !on(4, "run"),
            "a method is implemented per override, not per heritage"
        );
        assert!(!on(6, "make"), "a function");
        assert!(!on(0, "Other"), "another name on the line");
        assert!(!on(8, "Box"), "a line past the file");
    }

    #[test]
    fn a_base_spelled_outside_a_heritage_clause_names_no_subtype() {
        let source = "class Base {}\nconst x: Base = new Base();\n// class Z extends Base {}\n";
        assert!(subtypes(source, "/work/a.ts", "Base").is_empty());
    }

    /// The warning names as many unread files as the lead lists one row
    /// each, and counts the rest.
    #[test]
    fn the_unread_warning_names_a_few_and_counts_the_rest() {
        let unread = (1..=7).map(|n| format!("src/f{n}.ts")).collect::<Vec<_>>();
        let warning = describe_unread(&unread);
        assert!(warning.contains("7 file(s)"), "{warning}");
        assert!(warning.contains("src/f5.ts and 2 more."), "{warning}");
        assert!(!warning.contains("src/f6.ts"), "{warning}");
        assert!(describe_unread(&unread[..1]).ends_with("missing: src/f1.ts."));
    }

    /// The scan names every file it could not read, so the gap in a
    /// heritage answer can be followed.
    #[test]
    fn the_scan_names_the_files_it_could_not_read() {
        let dir = tempfile::tempdir().expect("workspace");
        let readable = dir.path().join("a.ts");
        std::fs::write(&readable, "class A extends Base {}\n").expect("source");
        let missing = dir.path().join("gone.ts").display().to_string();
        let files = BTreeSet::from([readable.display().to_string(), missing.clone()]);
        let (found, unread) = scan_subtypes(&files, "Base", &|| false);
        assert_eq!(found.len(), 1);
        assert_eq!(unread, vec![missing]);
    }

    #[test]
    fn a_syntax_fallback_note_discloses_direct_unverified_relations() {
        let failure = LspFailure::invalid_query("x");
        assert!(describe_scan(&failure, "Base", 3, "subtypes").contains("direct subtypes only"));
        let implementations = describe_scan(&failure, "Base", 3, "implementation");
        assert!(
            implementations.contains("direct implementations only")
                && implementations.contains("run implementation on each"),
            "{implementations}"
        );
    }

    /// A server that cannot answer (timeout, crash, rejected request) leaves
    /// the syntax scan the cheaper labelled answer; a cancelled request or a
    /// refused path does not.
    #[test]
    fn a_failed_implementation_request_has_the_syntax_scan_as_fallback() {
        use super::super::failure::LspFailure;
        use octocode_engine::error::{Error, ErrorKind};
        let failure = |kind| LspFailure::from_engine(&Error::with_kind(kind, "x"));
        assert!(implementation_failure_has_fallback(&failure(
            ErrorKind::Timeout
        )));
        assert!(implementation_failure_has_fallback(&failure(
            ErrorKind::ConnectionClosed
        )));
        assert!(implementation_failure_has_fallback(&failure(
            ErrorKind::Other
        )));
        assert!(!implementation_failure_has_fallback(
            &LspFailure::invalid_query("x")
        ));
    }

    /// The syntax scan runs while the server request is pending, so a failed
    /// request costs `max(request, scan)`, not their sum; an answer in time
    /// drops the scan; a failure with no fallback never waits for it.
    #[tokio::test]
    async fn the_hedge_overlaps_the_primary_and_yields_to_its_answer() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::time::{Duration, Instant};
        let sleep = |ms| tokio::time::sleep(Duration::from_millis(ms));

        let started = Instant::now();
        let outcome = hedged(
            async {
                sleep(300).await;
                Err::<u8, _>("timeout")
            },
            async {
                sleep(250).await;
                "scan"
            },
            |_| true,
            |_| None,
            |_| "overdue",
            false,
        )
        .await;
        assert_eq!(outcome, Err(("timeout", Some("scan"))));
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "sequential would take 550 ms, took {:?}",
            started.elapsed()
        );

        // A hedge slower than the failure is awaited to its end.
        let outcome = hedged(
            async { Err::<u8, _>("crash") },
            async {
                sleep(50).await;
                "scan"
            },
            |_| true,
            |_| None,
            |_| "overdue",
            false,
        )
        .await;
        assert_eq!(outcome, Err(("crash", Some("scan"))));

        // A failure with no fallback does not wait for the hedge.
        let started = Instant::now();
        let outcome = hedged(
            async { Err::<u8, _>("cancelled") },
            async {
                sleep(2_000).await;
                "scan"
            },
            |failure| *failure != "cancelled",
            |_| None,
            |_| "overdue",
            false,
        )
        .await;
        assert_eq!(outcome, Err(("cancelled", None)));
        assert!(started.elapsed() < Duration::from_millis(1_000));

        // An answer drops the hedge before it finishes.
        struct Dropped(Arc<AtomicBool>);
        impl Drop for Dropped {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let dropped = Arc::new(AtomicBool::new(false));
        let guard = Dropped(Arc::clone(&dropped));
        let outcome = hedged(
            async {
                sleep(20).await;
                Ok::<u8, &str>(7)
            },
            async move {
                let _guard = guard;
                sleep(5_000).await;
                "scan"
            },
            |_| true,
            |_| None,
            |_| "overdue",
            false,
        )
        .await;
        assert_eq!(outcome, Ok((7, None)));
        assert!(dropped.load(Ordering::SeqCst), "the answer drops the scan");

        // `keep` waits for the hedge and returns it beside the answer.
        let outcome = hedged(
            async { Ok::<u8, &str>(5) },
            async {
                sleep(50).await;
                "scan"
            },
            |_| true,
            |_| None,
            |_| "overdue",
            true,
        )
        .await;
        assert_eq!(outcome, Ok((5, Some("scan"))));
    }

    /// Once the scan has finished the server has a bounded grace: a primary
    /// answering inside it wins, one still silent after it yields the scan
    /// without waiting for the primary's own timeout.
    #[tokio::test]
    async fn a_server_silent_after_the_scan_loses_only_after_its_grace() {
        use std::time::{Duration, Instant};
        let sleep = |ms| tokio::time::sleep(Duration::from_millis(ms));
        let grace = |_: &&str| Some(Duration::from_millis(200));

        let started = Instant::now();
        let outcome = hedged(
            async {
                sleep(5_000).await;
                Ok::<u8, &str>(1)
            },
            async {
                sleep(50).await;
                "scan"
            },
            |_| true,
            grace,
            |_| "overdue",
            false,
        )
        .await;
        assert_eq!(outcome, Err(("overdue", Some("scan"))));
        assert!(
            started.elapsed() < Duration::from_millis(1_500),
            "took {:?}",
            started.elapsed()
        );

        // An answer inside the grace wins over the finished scan.
        let outcome = hedged(
            async {
                sleep(150).await;
                Ok::<u8, &str>(9)
            },
            async {
                sleep(50).await;
                "scan"
            },
            |_| true,
            grace,
            |_| "overdue",
            false,
        )
        .await;
        assert_eq!(outcome, Ok((9, None)));

        // A scan with nothing to offer grants no deadline.
        let outcome = hedged(
            async {
                sleep(300).await;
                Ok::<u8, &str>(3)
            },
            async {
                sleep(10).await;
                "empty"
            },
            |_| true,
            |_| None,
            |_| "overdue",
            false,
        )
        .await;
        assert_eq!(outcome, Ok((3, None)));
    }
}
