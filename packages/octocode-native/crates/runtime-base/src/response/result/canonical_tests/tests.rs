use super::*;
use crate::contracts::validate_built_query;

/// A row stating its defaults and the row omitting them are one query to
/// the contract's validator, and no Rust default of the runtime's typed
/// query differs from the contract's.
#[test]
fn every_dropped_default_parses_back_to_the_same_query() {
    let (mut compared, mut dropped) = (0, 0);
    for tool in ToolId::ALL {
        let name = tool.as_str();
        let found = variants(tool);
        assert!(
            found.skipped.is_empty(),
            "{name}: the sampler built no row for {:?}",
            found.skipped
        );
        // Every declared leaf form is compared, and the contract declares
        // at least one per top-level branch.
        let contract = crate::contracts::tool_contract(tool).expect("contract");
        let branches = ["oneOf", "anyOf"]
            .iter()
            .find_map(|key| contract["querySchema"][*key].as_array())
            .map_or(1, Vec::len);
        assert!(
            found.rows.len() >= branches.max(1) && found.rows.len() == found.declared,
            "{name}: {} variants compared of {} declared ({branches} top-level)",
            found.rows.len(),
            found.declared
        );
        for Sampled { stated, omitted } in found.rows {
            let validate = |row: &Value| {
                validate_built_query(name, row.clone())
                    .unwrap_or_else(|error| panic!("{name}: {row} is rejected: {error:?}"))
            };
            let mut canonical = stated.clone();
            crate::response::result::canonical_row(tool, &mut canonical);
            assert_eq!(
                validate(&stated),
                validate(&canonical),
                "{name}: dropping defaults from {stated} changed the query"
            );
            assert_eq!(
                validate(&stated),
                validate(&omitted),
                "{name}: stating defaults in {omitted} changed the query"
            );
            // The runtime reads a row only after the contract applied its
            // defaults, so an optional field with no Rust default may stay
            // unset when omitted. One that does carry a Rust default must
            // carry the contract's.
            let (stated_typed, omitted_typed) = (typed(tool, &stated), typed(tool, &omitted));
            for (field, rust_default) in omitted_typed.as_object().into_iter().flatten() {
                if omitted.get(field).is_none() && !rust_default.is_null() {
                    assert_eq!(
                        Some(rust_default),
                        stated.get(field),
                        "{name}: `{field}` defaults differently in Rust than in the contract"
                    );
                }
            }
            assert_eq!(
                typed(tool, &validate(&stated)),
                typed(tool, &validate(&canonical)),
                "{name}: the typed query changes when defaults are dropped from {stated}"
            );
            assert!(
                omitted_typed
                    .as_object()
                    .into_iter()
                    .flatten()
                    .all(|(field, value)| stated_typed.get(field) == Some(value)),
                "{name}: stating defaults in {omitted} changed a typed field"
            );
            dropped += usize::from(canonical != stated);
            compared += 1;
        }
    }
    assert!(
        compared >= ToolId::ALL.len(),
        "only {compared} variants compared"
    );
    assert!(dropped > 0, "no sampled row states a droppable default");
}
