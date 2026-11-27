//! The block declarations a file's hit lines declare, and the row that states them.
use super::*;
use crate::gh_search_code::code_output::declared_rows;
use crate::gh_search_code::cut::Cut;

const SOURCE: &str = "import { escape } from './escape'\n\nexport function getRouteRegex(\n  route: string\n) {\n  const parsed = getParametrizedRoute(route)\n  return parsed\n}\n\nconst getRouteRegexCache = new Map()\n\nexport class RouteRegexStore {\n  getRouteRegex(route: string) {\n    return getRouteRegex(\n      route\n    )\n  }\n}\n";

fn scanned(keyword: &str) -> (Vec<u32>, Vec<Declared>) {
    match scan(
        SOURCE.as_bytes(),
        "route-regex.ts",
        &[keyword.to_owned()],
        "",
        Window::default(),
        &Passthrough,
    ) {
        FileHits::Lines {
            matched, declared, ..
        } => (matched, declared),
        other => panic!("{other:?}"),
    }
}

/// Every listed declaration is the outline's own: the block declared on
/// that hit line, with the outline's kind, name and last line, so a read of
/// `line..=end_line` returns what `view:"symbols"` names. A hit that declares
/// nothing, or a one-line declaration, lists none.
#[test]
fn a_hit_line_states_the_block_it_declares_as_the_outline_does() {
    let (matched, declared) = scanned("getRouteRegex");
    let outline = octocode_tools_shared::declarations::file_document(SOURCE, "route-regex.ts")
        .expect("typescript outlines");
    let blocks = outline
        .declarations
        .iter()
        .filter(|item| item.range.end.line > item.range.start.line)
        .map(|item| Declared {
            hit: item.selection_range.start.line + 1,
            line: item.selection_range.start.line + 1,
            end_line: item.range.end.line + 1,
            kind: item.kind.clone(),
            name: item.name.clone(),
        })
        .collect::<Vec<_>>();
    assert!(!declared.is_empty(), "the function head is a hit");
    for listed in &declared {
        assert!(matched.contains(&listed.line), "{listed:?} is a hit line");
        assert!(blocks.contains(listed), "{listed:?} is not in {blocks:?}");
        assert!(listed.end_line > listed.line, "{listed:?} is a block");
    }
    // Every block whose name the keyword is, on a hit line, is listed once.
    let named = blocks
        .iter()
        .filter(|block| block.name.contains("getRouteRegex") && matched.contains(&block.line))
        .count();
    assert_eq!(declared.len(), named, "{declared:?} of {blocks:?}");
    assert!(
        declared.windows(2).all(|pair| pair[0].line < pair[1].line),
        "ascending, one a line: {declared:?}"
    );
    // A use and a one-line declaration are hits that list no block.
    assert!(matched.len() > declared.len(), "{matched:?} {declared:?}");
}

/// A cut row lists the declarations of the hits it lists, and no other.
#[test]
fn a_cut_lists_the_declarations_of_its_own_hits() {
    let whole = scan(
        SOURCE.as_bytes(),
        "route-regex.ts",
        &["getRouteRegex".to_owned()],
        "",
        Window::default(),
        &Passthrough,
    );
    let FileHits::Lines {
        matched, declared, ..
    } = &whole
    else {
        panic!("{whole:?}");
    };
    for start in 0..matched.len() {
        for end in start + 1..=matched.len() {
            let Some((cut, _)) = (Cut { start, end }).apply(&whole, Window::default()) else {
                continue;
            };
            let FileHits::Lines {
                matched: shown,
                declared: listed,
                ..
            } = cut
            else {
                panic!("a cut lists lines");
            };
            let expected = declared
                .iter()
                .filter(|declared| shown.contains(&declared.hit))
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(listed, expected, "{start}..{end}");
        }
    }
}

/// The response states each declaration as the line-first outline string
/// localSearch lists: the hit's line, then kind, name and the end line.
#[test]
fn the_row_states_declarations_line_first_before_its_hits() {
    use crate::gh_search_code::code_output::{Resolution, shape_files};
    let hits = scan(
        SOURCE.as_bytes(),
        "route-regex.ts",
        &["export function getRouteRegex".to_owned()],
        "",
        Window::default(),
        &Passthrough,
    );
    let FileHits::Lines { declared, .. } = &hits else {
        panic!("{hits:?}");
    };
    let declared = declared.clone();
    assert_eq!(declared.len(), 1, "{declared:?}");
    let shape = |fields: serde_json::Value| {
        let query: crate::gh_search_code::GhSearchCodeQuery =
            serde_json::from_value(serde_json::json!({
                "owner": "o", "repo": "r", "keywords": ["export function getRouteRegex"],
                "fields": fields
            }))
            .expect("query");
        let mut rows =
            vec![serde_json::json!({"owner": "o", "repo": "r", "path": "route-regex.ts"})];
        shape_files(
            &mut rows,
            &query,
            &octocode_runtime_base::policy::discovery::is_sensitive_repository_file,
            Some(Resolution {
                sha: None,
                reference: None,
                hits: vec![("a".repeat(40), hits.clone())],
                unlisted: Vec::new(),
            }),
        );
        let mut page = serde_json::json!({ "files": rows });
        octocode_tools_shared::gh_shared::pick_rows(
            &mut page,
            "files",
            crate::gh_search_code::CODE_ROW_ID,
            &octocode_tools_shared::gh_shared::field_names(&query.fields),
        );
        page["files"][0].take()
    };
    let row = shape(serde_json::json!([]));
    let keys = row.as_object().expect("row").keys().collect::<Vec<_>>();
    let at = |key: &str| keys.iter().position(|k| *k == key).expect(key);
    assert!(at("declarations") < at("matches"), "{keys:?}");
    assert_eq!(
        row["declarations"],
        serde_json::json!(declared_rows(&declared))
    );
    // As served: the response stage states the list as outline strings.
    let mut data = serde_json::Map::new();
    data.insert("files".into(), serde_json::json!([row]));
    octocode_runtime_base::response::hoist::apply_on(
        octocode_runtime_base::id::ToolId::GhSearchCode,
        &mut data,
    );
    let served = data["files"][0]["declarations"][0]
        .as_str()
        .unwrap_or_else(|| panic!("an outline string: {data:?}"));
    let (line, rest) = served
        .split_once(octocode_runtime_base::response::numbered::SEPARATOR)
        .expect("line first");
    assert_eq!(line, declared[0].line.to_string());
    assert!(rest.contains(&declared[0].name), "{served}");
    assert!(
        rest.ends_with(&format!("(end {})", declared[0].end_line)),
        "{served}"
    );
    // Declarations qualify listed lines: a pick that keeps the lines keeps
    // them, and a pick without `matches` lists none.
    assert_eq!(
        shape(serde_json::json!(["matches"]))["declarations"],
        row["declarations"]
    );
    for fields in [
        serde_json::json!(["matchedLines"]),
        serde_json::json!(["path"]),
    ] {
        assert!(
            shape(fields.clone()).get("declarations").is_none(),
            "{fields}"
        );
    }
}

const GUIDE: &str = "# Guide\n\nAn invitation opens a pipe.\n\n## Invitations\n\nSend an invitation.\n\n### Tokens\n\nAn invitation token.\n\n## Usage\n\n## Invitation limits\n";

/// A Markdown hit states the innermost section holding it, the one a
/// `block:true` read of that line returns: built from the shared sections,
/// one entry a section however many of its lines are hits, and none for a
/// section of one line.
#[test]
fn a_markdown_hit_states_its_innermost_section_once() {
    use octocode_tools_shared::source_view::sections::{innermost, sections};
    let hits = scan(
        GUIDE.as_bytes(),
        "docs/GUIDE.md",
        &["invitation".to_owned()],
        "",
        Window::default(),
        &Passthrough,
    );
    let FileHits::Lines {
        matched, declared, ..
    } = &hits
    else {
        panic!("{hits:?}");
    };
    let outline = sections(GUIDE, "docs/GUIDE.md").expect("markdown");
    let mut stated = 0;
    for hit in matched {
        let section = innermost(&outline, *hit as usize).expect("every line is in the guide");
        let listed = declared.iter().find(|declared| declared.hit == *hit);
        if section.end == section.line {
            assert!(
                listed.is_none(),
                "a one-line section reads as its hit: {hit}"
            );
            continue;
        }
        let listed = listed.unwrap_or_else(|| panic!("hit {hit} states {section:?}"));
        assert_eq!(
            (
                listed.line as usize,
                listed.end_line as usize,
                listed.name.as_str()
            ),
            (section.line, section.end, section.name.as_str()),
            "hit {hit}"
        );
        assert!(listed.line <= *hit && *hit <= listed.end_line, "{listed:?}");
        stated += 1;
    }
    assert_eq!(declared.len(), stated, "{declared:?}");
    // The row states each section once, by line.
    let rows = declared_rows(declared);
    let lines = rows
        .iter()
        .map(|row| row["line"].as_u64().expect("line"))
        .collect::<Vec<_>>();
    assert!(lines.windows(2).all(|pair| pair[0] < pair[1]), "{rows:?}");
    assert!(
        rows.len() < declared.len(),
        "two hits share a section: {rows:?}"
    );
    assert!(rows.iter().all(|row| row["kind"] == "section"), "{rows:?}");
}
