//! A split row names itself on every part but describes itself once: on
//! each object its paged collection passes through, a member that is not a
//! core-declared page-context field (`pageContextFields`) and holds no
//! declared collection rides the row's first part only, and the parts still
//! concatenate back to the unsplit row exactly.
use super::nested_continuation::{continued_elements, merge, symbols_row};
use super::*;
use crate::contracts::output_fields::page_context_fields;
use crate::id::ToolId;
use octocode_io::cancellation::Cancellation;

fn structure_row() -> Value {
    let entries = |dir: &str, count: usize| {
        (0..count)
            .map(|n| {
                format!(
                    "{dir}/f{n}.rs ({n}874, totalLines=5{n}, modified=2026-10-28T16:55:13.542Z)"
                )
            })
            .collect::<Vec<_>>()
    };
    json!({"path":"pkg",
        "files":[{"dir":"pkg","files":entries("pkg", 6)},{"dir":"pkg/sub","files":entries("pkg/sub", 3)}],
        "snapshot":"d433cf050ed9b93a689aaaa99f3ab28c612abfe6e7feff221e23173827b6e6b2",
        "summary":{"files":9,"dirs":2,"bytes":164_389}})
}

fn search_row() -> Value {
    let matches = |name: &str| {
        (1..=3)
            .map(|line| json!({"line": line * 5, "value": format!("fn {name}_{line}() {{}}")}))
            .collect::<Vec<_>>()
    };
    json!({"matchCount":6,"matchedLineCount":6,"filesScanned":2,"bytesSearched":3565,
        "files":[{"path":"src/a.rs","matches":matches("alpha")},{"path":"src/b.rs","matches":matches("beta")}]})
}

fn outline_row() -> Value {
    let mut data = symbols_row();
    data["operation"] = json!("symbols");
    data["path"] = json!("src");
    data
}

/// Walk `data` at `budget` and return its parts in order.
fn parts(tool: ToolId, data: &Value, budget: usize) -> Vec<Value> {
    let plan = PreparedRows::new(vec![json!({"index":0,"data":data.clone()})], &[None], tool)
        .plan(budget, &Cancellation::new())
        .expect("valid plan");
    (0..plan.total_pages)
        .flat_map(|page| {
            plan.build_page(page, &[None], &[], &Cancellation::new())
                .expect("valid page")
        })
        .collect()
}

#[test]
fn later_parts_carry_page_context_and_describe_the_row_once() {
    for (tool, data) in [
        (ToolId::StructureSearch, structure_row()),
        (ToolId::LocalSearch, search_row()),
        (ToolId::AstSearch, outline_row()),
    ] {
        let context = page_context_fields(tool);
        let declared =
            |name: &str| context.required.contains(name) || context.optional.contains(name);
        let mut split = false;
        for budget in (60..=400).step_by(5) {
            let parts = parts(tool, &data, budget);
            split |= parts.len() > 1;
            let mut joined: Option<Value> = None;
            for part in &parts {
                assert_eq!(
                    part["index"],
                    json!(0),
                    "{tool:?} budget {budget}: every part names its query"
                );
                let continued = continued_elements(&part["rowPart"]["continues"]);
                match joined.as_mut() {
                    None => joined = Some(part["data"].clone()),
                    Some(joined) => merge(joined, part["data"].clone(), "/data", &continued),
                }
            }
            assert_eq!(
                joined.as_ref(),
                Some(&data),
                "{tool:?} budget {budget}: parts join to the row"
            );
            for (name, value) in data.as_object().expect("data object") {
                if name == "files" {
                    continue;
                }
                let carried = parts
                    .iter()
                    .filter(|part| part["data"].get(name) == Some(value))
                    .count();
                let expected = if declared(name) { parts.len() } else { 1 };
                assert_eq!(
                    carried,
                    expected,
                    "{tool:?} budget {budget}: data.{name} on {carried} of {} parts",
                    parts.len()
                );
            }
        }
        assert!(split, "{tool:?}: some budget splits the row");
    }
}
