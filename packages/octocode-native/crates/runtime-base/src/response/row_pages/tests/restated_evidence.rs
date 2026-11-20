//! A split row states each fact once: a later part carries required
//! evidence (a count, an enum) only where the output schema needs it there,
//! and the parts, joined as the server instructions say (append each part
//! in order, merge the element `rowPart.continues` names, append to the
//! string `rowPart.text` names), rebuild the unsplit row's values exactly
//! once. Member order is layout, not evidence.
use super::nested_continuation::{Join, continued_elements, declared_collections, merge};
use super::part_metadata::parts;
use super::*;
use crate::id::ToolId;

fn search_row() -> Value {
    let files = (0..6)
        .map(|file| {
            let matches = (1..=8)
                .map(|line| json!({"line": line * 3, "value": format!("let needle_{file}_{line} = {line};")}))
                .collect::<Vec<_>>();
            json!({"path": format!("src/m{file}.rs"), "matches": matches})
        })
        .collect::<Vec<_>>();
    json!({"matchCount": 48, "matchedLineCount": 48, "matchedFiles": 6, "filesScanned": 9, "files": files})
}

fn fetch_row() -> Value {
    let content = (1..=60)
        .map(|line| format!("{line}\tconst value_{line} = compute({line});"))
        .collect::<Vec<_>>()
        .join("\n");
    json!({"path": "src/values.ts", "content": content, "totalLines": 60})
}

fn code_row() -> Value {
    let files = (0..5)
        .map(|file| {
            let matches = (1..=6)
                .map(|line| format!("{}\tfn handler_{file}_{line}() {{}}", line * 4))
                .collect::<Vec<_>>();
            json!({"path": format!("src/h{file}.rs"), "matches": matches})
        })
        .collect::<Vec<_>>();
    json!({"owner": "octo", "repo": "demo", "files": files})
}

/// Join `parts` (a row's parts, in order) as the instructions say.
fn joined(tool: ToolId, data: &Value, parts: &[Value]) -> Value {
    let collections = declared_collections(tool, data);
    let mut joined: Option<Value> = None;
    for part in parts {
        let continued = continued_elements(&part["rowPart"]["continues"]);
        match joined.as_mut() {
            None => joined = Some(part["data"].clone()),
            Some(joined) => merge(
                joined,
                part["data"].clone(),
                "/data",
                &Join {
                    continued: &continued,
                    text: part["rowPart"]["text"].as_str(),
                    collections: &collections,
                },
            ),
        }
    }
    joined.expect("a part")
}

/// Whether the result row `data` of `tool` validates against the output
/// schema.
fn valid(tool: ToolId, data: &Value) -> bool {
    let published = crate::test_support::published(tool, data);
    crate::contracts::validate_output(tool.as_str(), &published).is_ok()
}

#[test]
fn later_parts_restate_no_evidence_the_schema_does_not_need_and_join_exactly_once() {
    for (tool, data) in [
        (ToolId::LocalSearch, search_row()),
        (ToolId::LocalFetch, fetch_row()),
        (ToolId::GhSearchCode, code_row()),
    ] {
        assert!(valid(tool, &data), "{tool:?}: the fixture is a valid row");
        let context = crate::contracts::output_fields::page_context_fields(tool);
        let mut split = false;
        for budget in (80..=600).step_by(20) {
            let parts = parts(tool, &data, budget);
            split |= parts.len() > 1;
            // Equal values: a collection item delivered twice, or a value
            // a later part changed, makes the join differ.
            assert_eq!(
                joined(tool, &data, &parts),
                data,
                "{tool:?} budget {budget}: the parts join to the row exactly once"
            );
            for part in parts.iter().skip(1) {
                for (name, value) in part["data"].as_object().expect("part data") {
                    let pointer = format!("/data/{}", crate::json_pointer::escape(name));
                    if context.at(&pointer)
                        != Some(crate::contracts::output_fields::PageContext::RequiredEvidence)
                        || value.is_array()
                        || value.as_str().is_some_and(str::is_empty)
                    {
                        continue;
                    }
                    let mut without = part["data"].clone();
                    without
                        .as_object_mut()
                        .expect("part data")
                        .shift_remove(name);
                    assert!(
                        !valid(tool, &without),
                        "{tool:?} budget {budget}: a later part restates data.{name}, which it validates without"
                    );
                }
            }
        }
        assert!(split, "{tool:?}: some budget splits the row");
    }
    // The count the first part states is not repeated.
    let parts = parts(ToolId::LocalSearch, &search_row(), 200);
    assert!(parts.len() > 1, "the row splits");
    assert!(parts[0]["data"].get("matchCount").is_some());
    for part in &parts[1..] {
        assert!(part["data"].get("matchCount").is_none(), "{part}");
    }
}
