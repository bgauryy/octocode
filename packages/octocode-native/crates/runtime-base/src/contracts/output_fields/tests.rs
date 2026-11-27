use super::*;
use octocode_test_support::json_fixture;

/// A bounded value (the clasify answer `type` enum) is never free text;
/// file content is; a path the schema does not reach stays whole.
#[test]
fn free_text_follows_the_output_schema() {
    assert!(!free_text_at(ToolId::Clasify, "/data/answers/q0/type"));
    assert!(!free_text_at(ToolId::Clasify, "/data/questions/q0/type"));
    assert!(free_text_at(ToolId::GhGetFileContent, "/data/content"));
    assert!(!free_text_at(ToolId::GhGetFileContent, "/data/undeclared"));
}

/// Required evidence is emptied only where the output schema admits the
/// empty form: a score question's levels (at least two items) and a
/// choice question's labels (at least one property) repeat whole.
/// `instructions` is optional beside a `noul` question's `criteria`
/// (`clasify.noul-needs-one`), so the schema declares no repeat of it: a
/// later part leaves it out.
#[test]
fn emptying_honours_the_output_schema_bounds() {
    use serde_json::json;
    let clasify = page_context_fields(ToolId::Clasify);
    let later = |pointer: &str, value: serde_json::Value| {
        let mut kept = value;
        assert!(clasify.keep_later(pointer, &mut kept), "{pointer}");
        kept
    };
    for (pointer, value) in [
        (
            "/data/questions/q/criteria",
            json_fixture(r#"["low", "high"]"#),
        ),
        (
            "/data/questions/q/criteria",
            json_fixture(r#"{"yes": "holds"}"#),
        ),
    ] {
        assert_eq!(later(pointer, value.clone()), value, "{pointer}");
    }
    let mut instructions = json!("one named fact");
    assert!(
        !clasify.keep_later("/data/questions/q/instructions", &mut instructions),
        "an optional field the schema does not declare is not repeated"
    );
    // A section's byte pair and context are details fields, optional in
    // the schema: a later part leaves them out, they are stated once.
    for pointer in [
        "/data/resources/r/sections/0/target/bytes",
        "/data/resources/r/sections/0/context",
    ] {
        let mut kept = json_fixture(r#"[0, 40]"#);
        assert!(!clasify.keep_later(pointer, &mut kept), "{pointer}");
    }
}

/// A later part keeps every required member, so it validates against the
/// published output schema: for every tool and every required-evidence
/// pointer, a string the schema leaves unbounded is kept as `""`, and one
/// it bounds rides whole and unchanged.
#[test]
fn a_later_part_keeps_required_free_text_empty_for_every_tool() {
    let mut emptied = 0;
    for tool in ToolId::ALL {
        let contract = crate::contracts::tool_contract(tool).expect("embedded contract");
        let fields = page_context_fields(tool);
        for pointer in contract["pageContextFields"]["requiredEvidence"]
            .as_array()
            .expect("required evidence list")
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(|pointer| pointer.replace('*', "0"))
        {
            let text = serde_json::json!("evidence text");
            let mut later = text.clone();
            let kept = fields.keep_later(&pointer, &mut later);
            assert!(kept, "{tool:?} {pointer}: a required member stays");
            if emptiable_at(tool, &pointer, &text) {
                emptied += 1;
                assert_eq!(later, "", "{tool:?} {pointer}: free text empties");
            } else {
                assert!(kept, "{tool:?} {pointer}");
                assert_eq!(later, text, "{tool:?} {pointer}: whole when bounded");
            }
        }
    }
    assert!(emptied > 0, "some required free text is declared");
}

#[test]
fn every_tool_declares_each_page_context_pointer_once() {
    for tool in ToolId::ALL {
        let contract = crate::contracts::tool_contract(tool).expect("embedded contract");
        let declared = ["required", "requiredEvidence", "optional"]
            .iter()
            .flat_map(|kind| {
                contract["pageContextFields"][*kind]
                    .as_array()
                    .expect("page context list")
                    .iter()
                    .filter_map(serde_json::Value::as_str)
            })
            .collect::<Vec<_>>();
        let unique = declared.iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), declared.len(), "{tool:?}");
        assert_eq!(
            page_context_fields(tool).kinds.len(),
            declared.len(),
            "{tool:?}"
        );
    }
}

/// Page context names a member of one object, not every member of that
/// name: a related call's required `content` input leaves the row's
/// optional `path` first-part evidence, and a recursive member resolves
/// through its collection reference.
#[test]
fn page_context_resolves_by_row_pointer() {
    let fetch = page_context_fields(ToolId::LocalFetch);
    assert_eq!(
        fetch.at("/data/content"),
        Some(PageContext::RequiredEvidence)
    );
    assert_eq!(fetch.at("/data/path"), Some(PageContext::Optional));
    assert_eq!(fetch.at("/data/related/0/input/queries/0/content"), None);
    assert_eq!(fetch.at("/data/pagination"), Some(PageContext::Optional));
    let outline = page_context_fields(ToolId::AstSearch);
    assert_eq!(
        outline.at("/data/symbols/3/members/0/members/1/exported"),
        outline.at("/data/symbols/0/exported"),
    );
    assert_eq!(
        outline.at("/data/symbols/0/exported"),
        Some(PageContext::Optional)
    );
}

#[test]
fn optional_collection_slots_sit_at_fixed_places() {
    for tool in ToolId::ALL {
        let context = page_context_fields(tool);
        assert!(
            context
                .optional_paths
                .iter()
                .all(|path| !path.contains("/*")),
            "{tool:?}"
        );
    }
    for tool in [
        ToolId::LocalSearch,
        ToolId::StructureSearch,
        ToolId::AstSearch,
    ] {
        let context = page_context_fields(tool);
        assert!(context.optional_paths.contains("/data/skippedPaths"));
        assert!(
            !context
                .optional_paths
                .contains("/data/skippedPaths/entries")
        );
        assert!(!context.optional_paths.contains("/data/files"));
    }
}
