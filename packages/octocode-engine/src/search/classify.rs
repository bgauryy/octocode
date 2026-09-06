//! Tier 1 Phase 2 — deterministic AST match-kind classification.
//!
//! Given a file's content and the positions of text/regex matches, parse the
//! file once with tree-sitter and label each match with the kind of the
//! smallest named node covering it: declaration, import, export, callsite,
//! identifier, comment, string, config key, or heading. This is the AST signal
//! the tools-core ranker prefers over its regex line heuristics (Phase 1).
//!
//! Properties:
//!   * Stable labels: each completed classification depends on content + position.
//!   * Comment/string immune: a hit inside a comment or string is labeled as
//!     such, not as a declaration, because the label comes from the parse tree.
//!   * Optional + capped: only runs when `classify_matches` is set, and the
//!     caller bounds how many files are parsed and shares a two-second parsing
//!     budget across the classification pass. Unprocessed matches stay unlabeled.
//!   * Degrades gracefully: an unsupported extension or parse failure leaves
//!     matches unlabeled (kind = None), never an error.

use std::time::Instant;
use tree_sitter::Node;

use crate::signatures::extractor::{parse_before, AST_EXECUTION_TIMEOUT};
use crate::signatures::languages::find_entry;
use crate::types::{RipgrepFile, RipgrepMatch};

/// Default cap on how many files get parsed for classification per search, so
/// a broad query in a huge tree cannot pay an unbounded parse cost. Matches the
/// "classify top candidates first" rule in RANKING-ARCHITECTURE.md.
pub const DEFAULT_CLASSIFY_FILE_CAP: usize = 300;

/// Per-file size cap for classification. A matched file larger than this (e.g. a
/// minified/generated bundle) is skipped rather than read + parsed, mirroring
/// the structural search guard (`src/structural/files.rs`). Keeps a single broad
/// match from triggering an unbounded read + tree-sitter parse.
pub const DEFAULT_CLASSIFY_MAX_FILE_BYTES: u64 = 1_000_000;

fn extension_of(path: &str) -> &str {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    match name.rfind('.') {
        Some(i) if i + 1 < name.len() => &name[i + 1..],
        _ => "",
    }
}

/// Annotate matches across the first `cap` files in place by reading each file
/// and classifying its match positions. Files past the cap, unsupported
/// extensions, unreadable/unparseable files, and work past the shared deadline
/// are left unlabeled. This optional ranking budget never removes search hits.
pub fn classify_ripgrep_files(files: &mut [RipgrepFile], cap: usize) {
    let deadline = Instant::now() + AST_EXECUTION_TIMEOUT;
    for file in files.iter_mut().take(cap) {
        if Instant::now() >= deadline {
            break;
        }
        if file.matches.is_empty() {
            continue;
        }
        let ext = extension_of(&file.path);
        if find_entry(ext).is_none() {
            continue;
        }
        // Size guard before reading: skip large/minified files (parsing them is
        // expensive and they are low-signal for ranking anyway).
        match std::fs::metadata(&file.path) {
            Ok(meta) if meta.len() > DEFAULT_CLASSIFY_MAX_FILE_BYTES => continue,
            Ok(_) => {}
            Err(_) => continue,
        }
        let Ok(content) = std::fs::read_to_string(&file.path) else {
            continue;
        };
        classify_file_matches_before(&content, ext, &mut file.matches, deadline);
    }
}

/// Stable kind labels shared with the tools-core ranker.
pub const KIND_DECLARATION: &str = "declaration";
pub const KIND_IMPORT: &str = "import";
pub const KIND_EXPORT: &str = "export";
pub const KIND_CALLSITE: &str = "callsite";
pub const KIND_IDENTIFIER: &str = "identifier";
pub const KIND_COMMENT: &str = "comment";
pub const KIND_STRING: &str = "string";
pub const KIND_CONFIG_KEY: &str = "configKey";
pub const KIND_HEADING: &str = "heading";

/// Deterministic relevance hint per kind (0.0..1.0). The ranker may use this
/// directly or map the kind itself; keeping it here centralizes the policy.
fn score_hint_for(kind: &str) -> f64 {
    match kind {
        KIND_DECLARATION => 1.0,
        KIND_EXPORT => 0.9,
        KIND_CONFIG_KEY => 0.6,
        KIND_HEADING => 0.6,
        KIND_IMPORT => 0.5,
        KIND_CALLSITE => 0.4,
        KIND_IDENTIFIER => 0.2,
        // comments / strings are weak evidence for code relevance
        _ => 0.0,
    }
}

/// Classify every match in `matches` in place. No-op when the extension has no
/// grammar or the file does not parse.
#[cfg(test)]
pub fn classify_file_matches(content: &str, ext: &str, matches: &mut [RipgrepMatch]) {
    classify_file_matches_before(
        content,
        ext,
        matches,
        Instant::now() + AST_EXECUTION_TIMEOUT,
    );
}

fn classify_file_matches_before(
    content: &str,
    ext: &str,
    matches: &mut [RipgrepMatch],
    deadline: Instant,
) {
    let Some(entry) = find_entry(ext) else {
        return;
    };
    let Some(tree) = parse_before(content, &entry.language, deadline) else {
        return;
    };
    let root = tree.root_node();
    let line_starts = line_start_offsets(content);

    for m in matches.iter_mut() {
        if Instant::now() >= deadline {
            break;
        }
        let Some(byte) = position_to_byte(content, &line_starts, m.line, m.column) else {
            continue;
        };
        if let Some(node) = root.descendant_for_byte_range(byte, byte) {
            let kind = classify_node(node, content);
            m.score_hint = Some(score_hint_for(kind));
            m.kind = Some(kind.to_string());
        }
    }
}

/// Map a tree-sitter node (plus a few ancestors) to a stable kind label.
fn classify_node(node: Node, src: &str) -> &'static str {
    // 0. Config keys first: in JSON/YAML the key is itself a string node, so
    //    this must win over the string-lexical check below.
    {
        let mut cur = Some(node);
        let mut depth = 0;
        while let Some(n) = cur {
            if is_config_key(n, src) {
                return KIND_CONFIG_KEY;
            }
            if depth >= 2 {
                break;
            }
            depth += 1;
            cur = n.parent();
        }
    }

    // 1. Leaf-level lexical categories win next — these are the comment/string
    //    immunity guarantees and must not be overridden by enclosing structure.
    let mut cur = Some(node);
    let mut depth = 0;
    while let Some(n) = cur {
        let k = n.kind();
        if is_comment_kind(k) {
            return KIND_COMMENT;
        }
        if is_string_kind(k) {
            return KIND_STRING;
        }
        // Only inspect the immediate lexical neighborhood for comment/string.
        if depth >= 2 {
            break;
        }
        depth += 1;
        cur = n.parent();
    }

    // 2. Structural categories from the node and its ancestors.
    let mut cur = Some(node);
    let mut depth = 0;
    while let Some(n) = cur {
        let k = n.kind();
        if is_import_kind(k) {
            return KIND_IMPORT;
        }
        if is_export_kind(k) {
            return KIND_EXPORT;
        }
        if is_call_kind(k) {
            return KIND_CALLSITE;
        }
        if is_config_key(n, src) {
            return KIND_CONFIG_KEY;
        }
        if is_heading_kind(k) {
            return KIND_HEADING;
        }
        if is_declaration_kind(k) {
            return KIND_DECLARATION;
        }
        if depth >= 4 {
            break;
        }
        depth += 1;
        cur = n.parent();
    }

    KIND_IDENTIFIER
}

fn is_comment_kind(k: &str) -> bool {
    k.contains("comment")
}

fn is_string_kind(k: &str) -> bool {
    // string, string_literal, raw_string_literal, interpreted_string_literal,
    // template_string, char_literal …
    k.contains("string") || k.contains("char_literal") || k == "string_content"
}

fn is_import_kind(k: &str) -> bool {
    k.contains("import") || k == "use_declaration" || k == "use_clause" || k == "preproc_include"
}

fn is_export_kind(k: &str) -> bool {
    k.contains("export")
}

fn is_call_kind(k: &str) -> bool {
    k == "call_expression" || k == "call" || k == "function_call" || k == "method_invocation"
}

fn is_heading_kind(k: &str) -> bool {
    // markdown grammar (when present); harmless elsewhere.
    k.contains("heading") || k == "atx_heading" || k == "setext_heading"
}

/// JSON/YAML object key: the node is (or sits under) a pair/mapping key.
fn is_config_key(node: Node, _src: &str) -> bool {
    if let Some(parent) = node.parent() {
        let pk = parent.kind();
        if (pk == "pair" || pk == "block_mapping_pair" || pk == "flow_pair")
            && is_key_child(parent, node)
        {
            return true;
        }
    }
    false
}

fn is_key_child(parent: Node, child: Node) -> bool {
    // The key is the named field "key" where the grammar exposes it, else the
    // first named child.
    if let Some(key) = parent.child_by_field_name("key") {
        return key.id() == child.id();
    }
    parent
        .named_child(0)
        .map(|c| c.id() == child.id())
        .unwrap_or(false)
}

fn is_declaration_kind(k: &str) -> bool {
    // Cross-grammar declaration/definition node kinds.
    k.ends_with("_declaration")
        || k.ends_with("_definition")
        || k.ends_with("_item") // rust: function_item, struct_item, enum_item, …
        || k == "class_definition"
        || k == "function_definition"
        || k == "method_definition"
        || k == "function_declaration"
        || k == "class_declaration"
        || k == "interface_declaration"
        || k == "type_alias_declaration"
        || k == "decorated_definition"
        || k == "trait_item"
        || k == "impl_item"
}

fn line_start_offsets(content: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (i, b) in content.bytes().enumerate() {
        if b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

/// Convert a 1-based line + 0-based char column to a byte offset, clamped.
fn position_to_byte(content: &str, line_starts: &[usize], line: u32, column: u32) -> Option<usize> {
    if line == 0 {
        return None;
    }
    let line_idx = (line - 1) as usize;
    let line_start = *line_starts.get(line_idx)?;
    let line_end = line_starts
        .get(line_idx + 1)
        .copied()
        .unwrap_or(content.len());
    let slice = &content[line_start..line_end];
    // Walk `column` chars into the line, clamped to the line's content.
    let mut byte = line_start;
    for (chars, (off, _)) in slice.char_indices().enumerate() {
        if chars >= column as usize {
            byte = line_start + off;
            break;
        }
        byte = line_start + off;
    }
    Some(byte.min(content.len().saturating_sub(1)).max(line_start))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(line: u32, column: u32) -> RipgrepMatch {
        RipgrepMatch {
            line,
            column,
            value: String::new(),
            count: None,
            kind: None,
            score_hint: None,
        }
    }

    fn classify_one(content: &str, ext: &str, line: u32, col: u32) -> Option<String> {
        let mut ms = vec![m(line, col)];
        classify_file_matches(content, ext, &mut ms);
        ms[0].kind.clone()
    }

    #[test]
    fn ts_declaration_vs_comment_vs_string() {
        let src = "// fallback here\nexport function fallback() {}\nconst s = \"fallback\";\n";
        // line 1: comment
        assert_eq!(classify_one(src, "ts", 1, 3).as_deref(), Some(KIND_COMMENT));
        // line 2 col 16: the `fallback` identifier in the declaration -> export/declaration
        let k = classify_one(src, "ts", 2, 16);
        assert!(
            matches!(k.as_deref(), Some(KIND_EXPORT) | Some(KIND_DECLARATION)),
            "got {k:?}"
        );
        // line 3: inside a string literal
        assert_eq!(classify_one(src, "ts", 3, 11).as_deref(), Some(KIND_STRING));
    }

    #[test]
    fn rust_declaration_and_call() {
        let src = "pub fn handler() {\n    other();\n}\n";
        let decl = classify_one(src, "rs", 1, 7);
        assert!(
            matches!(decl.as_deref(), Some(KIND_DECLARATION) | Some(KIND_EXPORT)),
            "got {decl:?}"
        );
        assert_eq!(
            classify_one(src, "rs", 2, 4).as_deref(),
            Some(KIND_CALLSITE)
        );
    }

    #[test]
    fn import_lines() {
        let ts = "import { foo } from 'bar';\n";
        assert_eq!(classify_one(ts, "ts", 1, 9).as_deref(), Some(KIND_IMPORT));
        let py = "import os\n";
        assert_eq!(classify_one(py, "py", 1, 7).as_deref(), Some(KIND_IMPORT));
    }

    #[test]
    fn expired_classification_keeps_matches_unlabeled() {
        let mut matches = vec![m(1, 0)];
        classify_file_matches_before("import os\n", "py", &mut matches, Instant::now());
        assert_eq!(matches[0].kind, None);
        assert_eq!(matches[0].score_hint, None);
    }

    #[test]
    fn unsupported_extension_is_noop() {
        let mut ms = vec![m(1, 0)];
        classify_file_matches("whatever", "unknownext", &mut ms);
        assert_eq!(ms[0].kind, None);
    }

    #[test]
    fn oversized_file_is_skipped_not_parsed() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("octocode-classify-cap-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.ts");
        // Write just over the cap so the size guard trips before read/parse.
        let mut f = std::fs::File::create(&path).unwrap();
        let line = "export function fallback() {}\n";
        let mut written = 0u64;
        while written <= DEFAULT_CLASSIFY_MAX_FILE_BYTES {
            f.write_all(line.as_bytes()).unwrap();
            written += line.len() as u64;
        }
        drop(f);

        let mut files = vec![RipgrepFile {
            path: path.to_string_lossy().into_owned(),
            match_count: 1,
            matches: vec![m(1, 16)],
        }];
        classify_ripgrep_files(&mut files, DEFAULT_CLASSIFY_FILE_CAP);
        // Skipped by the size guard -> match left unlabeled.
        assert_eq!(files[0].matches[0].kind, None);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn json_config_key() {
        let src = "{\n  \"handler\": \"build\"\n}\n";
        let k = classify_one(src, "json", 2, 3);
        assert_eq!(k.as_deref(), Some(KIND_CONFIG_KEY));
    }

    #[test]
    fn config_values_are_not_misclassified_as_keys() {
        for (ext, src) in [
            ("json", r#"{"handler": "build"}"#),
            ("json", r#"{"handler": {"child": "build"}}"#),
            ("yaml", "handler: build\n"),
            ("yaml", "handler: \"build\"\n"),
            ("yaml", "{handler: build}\n"),
        ] {
            let key = src.find("handler").expect("key fixture") as u32;
            let value = src.find("build").expect("value fixture") as u32;
            assert_eq!(
                classify_one(src, ext, 1, key).as_deref(),
                Some(KIND_CONFIG_KEY)
            );
            let value_kind = classify_one(src, ext, 1, value);
            assert_ne!(value_kind.as_deref(), Some(KIND_CONFIG_KEY), "{src}");
            if ext == "json" {
                assert_eq!(value_kind.as_deref(), Some(KIND_STRING));
            }
        }
    }
}
