//! Workspace-symbol rows whose site is an import or re-export item.
//!
//! A server reports `pub use a::{Name};` with the kind of its target, so the
//! row reads as a second declaration of `Name`. The source line the row
//! points at decides it: the row stays (it is evidence) and one warning
//! names the rows that are not declarations.

use super::*;
use crate::lsp_search::source::Source;

/// Statement-group lines scanned back from a row for the `use` that opens it.
const GROUP_LOOKBACK: usize = 64;

/// Words that make a line a declaration, never a `use` group member.
const RUST_DECLARATION_WORDS: &[&str] = &[
    "fn",
    "struct",
    "enum",
    "trait",
    "type",
    "const",
    "static",
    "mod",
    "impl",
    "union",
    "let",
    "macro_rules",
    "pub",
    "extern",
];

fn rust_use_start(line: &str) -> bool {
    let line = line.trim_start();
    let Some(rest) = line.strip_prefix("pub") else {
        return line.starts_with("use ") || line.starts_with("use{");
    };
    let rest = rest.trim_start();
    let rest = match rest.strip_prefix('(') {
        Some(inner) => inner
            .split_once(')')
            .map_or("", |(_, after)| after.trim_start()),
        None => rest,
    };
    rest.starts_with("use ") || rest.starts_with("use{")
}

/// A line of a multi-line `use` group: paths, names, commas and braces only.
fn rust_group_member(line: &str) -> bool {
    let line = line.trim();
    !line.is_empty()
        && line
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | ':' | ',' | ' ' | '{' | '}' | ';'))
        && !line
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .any(|word| RUST_DECLARATION_WORDS.contains(&word))
}

fn rust_import_site(source: &Source, line: usize) -> bool {
    let at = |index: usize| source.lines(index, index + 1);
    let first = line.saturating_sub(1);
    if rust_use_start(at(first)) {
        return true;
    }
    let mut index = first;
    for _ in 0..GROUP_LOOKBACK {
        if !rust_group_member(at(index)) {
            return false;
        }
        if index == 0 {
            return false;
        }
        index -= 1;
        if rust_use_start(at(index)) {
            return true;
        }
    }
    false
}

fn script_import_site(text: &str) -> bool {
    let text = text.trim_start();
    text.starts_with("import ")
        || text.starts_with("import{")
        || (text.starts_with("export")
            && (text.contains(" from ") || text.contains("}from"))
            && !text.starts_with("export const ")
            && !text.starts_with("export function "))
}

fn python_import_site(text: &str) -> bool {
    let text = text.trim_start();
    text.starts_with("import ") || (text.starts_with("from ") && text.contains(" import "))
}

/// Whether `line` (1-based) of `source`, a file of `path`, is an import or
/// re-export item (Rust `use`, TS/JS `import`/`export … from`, Python
/// `import`/`from … import`). Other languages: never.
pub(super) fn is_import_site(path: &str, source: &Source, line: usize) -> bool {
    if line == 0 {
        return false;
    }
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("");
    let text = source.lines(line - 1, line);
    match extension {
        "rs" => rust_import_site(source, line),
        "ts" | "tsx" | "mts" | "cts" | "js" | "jsx" | "mjs" | "cjs" => script_import_site(text),
        "py" | "pyi" => python_import_site(text),
        _ => false,
    }
}

/// The `path:line` of each listed row that is an import or re-export item.
pub(super) async fn import_rows(sources: &mut SourceCache<'_>, rows: &[Value]) -> Vec<String> {
    let mut found = Vec::new();
    for row in rows {
        let (Some(path), Some(line)) = (
            row.get("path").and_then(Value::as_str),
            row.get("line").and_then(Value::as_u64),
        ) else {
            continue;
        };
        let Some(source) = sources.get(path).await else {
            continue;
        };
        if is_import_site(path, &source, octocode_engine::num::saturating_usize(line)) {
            found.push(format!("{path}:{line}"));
        }
    }
    found
}

/// The warning naming `rows`, or `None` when there are none.
pub(super) fn warning(rows: &[String]) -> Option<String> {
    if rows.is_empty() {
        return None;
    }
    Some(format!(
        "{} listed row(s) sit on an import or re-export line, not a declaration, though their kind is the target's: {}.",
        rows.len(),
        rows.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(path: &str, text: &str, line: usize) -> bool {
        is_import_site(path, &Source::new(text.to_owned()), line)
    }

    #[test]
    fn rust_use_items_are_import_sites_and_declarations_are_not() {
        let text = "pub use repository::{FailedRepository, RepoFacts, Other};\n\nstruct RepoFacts {\n    a: u8,\n}\npub(crate) use x::y;\nuse a::{\n    One,\n    Two as Three,\n};\nfn Two() {}\n";
        assert!(site("a.rs", text, 1));
        assert!(!site("a.rs", text, 3));
        assert!(!site("a.rs", text, 4));
        assert!(site("a.rs", text, 6));
        assert!(site("a.rs", text, 8));
        assert!(site("a.rs", text, 9));
        assert!(!site("a.rs", text, 11));
    }

    #[test]
    fn script_and_python_imports_are_import_sites() {
        assert!(site(
            "a.ts",
            "export { A } from './a';\nexport const B = 1;\n",
            1
        ));
        assert!(!site(
            "a.ts",
            "export { A } from './a';\nexport const B = 1;\n",
            2
        ));
        assert!(site("a.js", "import { A } from 'a';\n", 1));
        assert!(site("a.py", "from a import B\nclass C: pass\n", 1));
        assert!(!site("a.py", "from a import B\nclass C: pass\n", 2));
        assert!(!site("a.go", "import \"x\"\n", 1));
    }

    #[test]
    fn the_warning_names_each_import_row_and_is_absent_without_one() {
        assert_eq!(warning(&[]), None);
        let text = warning(&["/w/a.rs:41".into(), "/w/b.rs:2".into()]).expect("warning");
        assert!(
            text.contains("/w/a.rs:41") && text.contains("/w/b.rs:2"),
            "{text}"
        );
    }
}
