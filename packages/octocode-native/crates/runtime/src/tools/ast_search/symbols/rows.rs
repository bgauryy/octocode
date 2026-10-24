use super::super::RowPaths;
use crate::response::symbol_outline::declaration_rows;
use serde_json::{Value, json};

/// Characters each top-level row (a span of `declarations`) adds to its
/// page, as the page delivers it: its rendered row with its members, without
/// the `path` a directory page states once per file ([`group_by_file`]), plus
/// that file's `{path, symbols}` wrapper (its path as the response prints
/// it, [`RowPaths`]) the first time the file appears. `directory` holds the
/// row paths of a directory outline; a file outline names no file per row.
pub(super) fn row_sizes(
    declarations: &[Value],
    spans: &[std::ops::Range<usize>],
    directory: Option<RowPaths<'_>>,
) -> Vec<usize> {
    let mut last_path: Option<&str> = None;
    spans
        .iter()
        .map(|span| {
            let rows = &declarations[span.clone()];
            let rendered = if directory.is_some() {
                let unpathed = rows
                    .iter()
                    .map(|row| {
                        let mut row = row.clone();
                        if let Some(fields) = row.as_object_mut() {
                            fields.remove("path");
                        }
                        row
                    })
                    .collect::<Vec<_>>();
                declaration_rows(&unpathed)
            } else {
                declaration_rows(rows)
            };
            // Each row is one array element: its text and a separator.
            let mut chars = rendered
                .iter()
                .map(|row| octocode_engine::json_size::utf16_units(row).unwrap_or(0) + 1)
                .sum::<usize>();
            if let Some(row_paths) = directory {
                let path = declarations[span.start]["path"].as_str();
                if path != last_path {
                    chars += path.map_or(0, |path| row_paths.chars(path)) + FILE_WRAPPER_CHARS;
                    last_path = path;
                }
            }
            chars
        })
        .collect()
}

/// The `{"path":,"symbols":[]},` around one file's rows on a directory page.
const FILE_WRAPPER_CHARS: usize = 24;

/// Consecutive rows of one file become `{path, declarations}` with outline
/// rows; rows arrive in path order, so each file appears once per page.
pub(super) fn group_by_file(rows: &[Value], row_paths: RowPaths<'_>) -> Vec<Value> {
    let mut files: Vec<(Value, Vec<Value>)> = Vec::new();
    for row in rows {
        let mut row = row.clone();
        let path = row
            .as_object_mut()
            .and_then(|fields| fields.remove("path"))
            .unwrap_or(Value::Null);
        match files.last_mut() {
            Some((seen, list)) if *seen == path => list.push(row),
            _ => files.push((path, vec![row])),
        }
    }
    files
        .into_iter()
        .map(|(path, rows)| json!({"path":path.as_str().map(|path| row_paths.delivered(path)),"symbols":declaration_rows(&rows)}))
        .collect()
}

/// The source lines of `spans` (1-based, inclusive), joined.
pub(super) fn error_text(source: &str, spans: &[[u32; 2]]) -> String {
    let lines = source.lines().collect::<Vec<_>>();
    spans
        .iter()
        .filter_map(|[start, end]| {
            let start = (*start as usize).max(1) - 1;
            let end = (*end as usize).min(lines.len());
            lines.get(start..end)
        })
        .flatten()
        .copied()
        .collect::<Vec<_>>()
        .join("\n")
}

/// A declaration's shown name: a Rust trait impl reads `Trait for Type` (as
/// rust-analyzer and the source spell it), so the type's impl blocks are
/// told apart; every other declaration keeps its name.
fn display_name(d: &Value) -> std::borrow::Cow<'_, str> {
    let name = d["name"].as_str().unwrap_or("");
    match d["implements"].as_str() {
        Some(implemented) => format!("{implemented} for {name}").into(),
        None => name.into(),
    }
}

/// Mark each Rust `impl Trait for Type` declaration with the trait it
/// implements (`implements`, from the extractor's syntactic heritage edge),
/// for [`display_name`].
pub(super) fn name_impl_traits<'a>(
    declarations: &mut [Value],
    edges: impl Iterator<Item = (&'a str, &'a str, &'a str)>,
) {
    let traits: std::collections::HashMap<&str, &str> = edges
        .filter(|(_, relation, _)| *relation == "implements")
        .map(|(from, _, to)| (from, to))
        .collect();
    if traits.is_empty() {
        return;
    }
    for declaration in declarations {
        if declaration["kind"] == "impl"
            && let Some(implemented) = declaration["id"].as_str().and_then(|id| traits.get(id))
        {
            declaration["implements"] = json!(implemented);
        }
    }
}

/// Projects engine declaration facts (one file) onto compact response rows.
///
/// `line` (1-based) anchors the declaration NAME, so a row feeds lspSearch as
/// `symbolName`+`line`; `character` (1-based) appears only when another
/// declaration of the same kind shares that name and line, so the column is
/// the only difference. `endLine` (1-based) ends the declaration and is omitted when
/// equal to `line`; `startLine` appears only when the declaration starts
/// before its name line (decorators, attributes); `docStartLine` is the first
/// line of the comment block directly above, when present. `parent` names the
/// containing declaration and stays meaningful when a `symbolKinds` or `name`
/// filter drops the parent row; `parentLine` is added only when another
/// declaration in the file has the same name and kind (two `impl A` blocks). `exported` appears only when true, with
/// `exportedAs` listing public names that differ from `name`. Rows are
/// returned in input order, one per engine declaration.
pub(super) fn compact_declarations(raw: &[Value]) -> Vec<Value> {
    let names: Vec<std::borrow::Cow<'_, str>> = raw.iter().map(display_name).collect();

    let pos = |d: &Value, range: &str, edge: &str, field: &str| {
        d.pointer(&format!("/{range}/{edge}/{field}"))
            .and_then(Value::as_u64)
    };
    let anchors: Vec<(&str, u64, u64)> = raw
        .iter()
        .enumerate()
        .map(|(index, d)| {
            let anchor = if d.get("selectionRange").is_some() {
                "selectionRange"
            } else {
                "range"
            };
            let line = pos(d, anchor, "start", "line")
                .map(|l| l + 1)
                .or_else(|| d["line"].as_u64())
                .unwrap_or(0);
            let character = pos(d, anchor, "start", "character").unwrap_or(0);
            (names[index].as_ref(), line, character)
        })
        .collect();
    let mut per_name_kind = std::collections::HashMap::<(&str, &str), usize>::new();
    let mut per_name_line = std::collections::HashMap::<(&str, u64, &str), usize>::new();
    for (d, (name, line, _)) in raw.iter().zip(&anchors) {
        let kind = d["kind"].as_str().unwrap_or("");
        *per_name_kind.entry((name, kind)).or_default() += 1;
        *per_name_line.entry((name, *line, kind)).or_default() += 1;
    }
    let by_engine_id: std::collections::HashMap<&str, usize> = raw
        .iter()
        .enumerate()
        .filter_map(|(index, d)| Some((d["id"].as_str()?, index)))
        .collect();
    raw.iter()
        .zip(&anchors)
        .map(|(d, &(name, line, character))| {
            let kind = d["kind"].as_str().unwrap_or("");
            let mut row = json!({"name":name,"kind":kind,"line":line});
            if d.get("implements").is_some() {
                row["typeName"] = d["name"].clone();
            }
            if per_name_line.get(&(name, line, kind)).copied().unwrap_or(0) > 1 {
                // 1-based, like every column.
                row["character"] = json!(character + 1);
            }
            if let Some(start) = pos(d, "range", "start", "line").map(|l| l + 1)
                && start < line
            {
                row["startLine"] = json!(start);
            }
            if let Some(doc) = d["docLine"].as_u64() {
                row["docStartLine"] = json!(doc + 1);
            }
            if let Some(end) = pos(d, "range", "end", "line").map(|l| l + 1)
                && end != line
            {
                row["endLine"] = json!(end);
            }
            if d["exported"].as_bool() == Some(true) {
                row["exported"] = json!(true);
                // Public names when exported under another name (`export { foo
                // as bar }`, `export default function foo`).
                if let Some(public) = d.get("exportedAs").filter(|v| v.is_array()) {
                    row["exportedAs"] = public.clone();
                }
            }
            // Parents precede children in engine preorder; an unknown parent
            // (never expected) is dropped rather than leaking an engine id.
            // A declaration never names itself as parent: C's one-line
            // `typedef struct X X;` holds `struct X` under `type X` at the
            // same name line, so the struct is the typedef's sibling and
            // takes the typedef's own parent.
            let mut parent_index = d["parent"]
                .as_str()
                .and_then(|id| by_engine_id.get(id))
                .copied();
            while let Some(index) = parent_index
                && anchors
                    .get(index)
                    .is_some_and(|&(parent, parent_line, _)| (parent, parent_line) == (name, line))
            {
                parent_index = raw[index]["parent"]
                    .as_str()
                    .and_then(|id| by_engine_id.get(id))
                    .copied();
            }
            if let Some(index) = parent_index
                && let Some(&(parent, parent_line, _)) = anchors.get(index)
            {
                row["parent"] = json!(parent);
                let parent_kind = raw[index]["kind"].as_str().unwrap_or("");
                if per_name_kind
                    .get(&(parent, parent_kind))
                    .copied()
                    .unwrap_or(0)
                    > 1
                {
                    row["parentLine"] = json!(parent_line);
                }
            }
            row
        })
        .collect()
}
