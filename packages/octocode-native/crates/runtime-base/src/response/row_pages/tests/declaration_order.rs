use super::*;
use octocode_test_support::json_fixture;

/// A localSearch file's `declarations` page like its hits: a narrow page
/// holds only the declarations that fit it, they arrive before the hits
/// naming them (a hit's `enclosing` indexes a declaration delivered on its
/// part or an earlier one), and the parts join back to the row. Kept whole on
/// part one, nine declarations made a page of twice an 800-char budget.
#[test]
fn a_files_declarations_split_across_parts_before_its_hits() {
    use super::evidence_closure::chars;
    use super::required_text::{assert_joins_to, walk_and_join};
    let tool = crate::id::ToolId::LocalSearch;
    let declarations = (0..9)
        .map(|n| {
            json!({"symbolName": format!("a_test_whose_name_says_what_it_checks_{n}"),
                "kind": "function", "line": n * 40 + 1, "endLine": n * 40 + 30})
        })
        .collect::<Vec<_>>();
    let matches = (0..9)
        .map(|n| json!({"line": n * 40 + 5, "value": format!("    keep_lead(tool_{n});"), "enclosing": n}))
        .collect::<Vec<_>>();
    let data = json!({"matchCount": 9, "filesScanned": 1, "files": [{
        "path": "src/response/workflow_lead_tests.rs",
        "declarations": declarations, "matches": matches}]});
    for budget in [400, 600, 800] {
        let (parts, joined) = walk_and_join(tool, &data, budget);
        assert_joins_to(&joined, &data, &format!("budget {budget}"));
        let mut delivered = 0;
        for (at, part) in parts.iter().enumerate() {
            assert!(
                chars(part) <= budget,
                "budget {budget}: part {at} holds {} chars: {part}",
                chars(part)
            );
            let file = &part["data"]["files"][0];
            delivered += file["declarations"].as_array().map_or(0, Vec::len);
            for hit in file["matches"].as_array().into_iter().flatten() {
                let owner = hit["enclosing"].as_u64().expect("enclosing");
                assert!(
                    usize::try_from(owner).expect("index") < delivered,
                    "budget {budget}: part {at} names declaration {owner} before it arrives"
                );
            }
        }
    }
}

#[test]
fn collection_order_follows_document_members_independent_of_routes() {
    use crate::contracts::output_fields::CollectionRoutes;
    let routes = CollectionRoutes::new(
        &["/data/owners/*/z~1~0".into(), "/definitions/items".into()],
        &[
            ("/data/owners/*/10".into(), "/definitions/hop".into()),
            ("/definitions/hop".into(), "/definitions/items".into()),
            ("/definitions/items".into(), "/definitions/hop".into()),
        ],
    );
    for (first, last) in [("z/~", "10"), ("10", "z/~")] {
        for object_owner in [true, false] {
            let mut siblings = Map::new();
            siblings.insert(first.into(), json_fixture(r#"["first-a", "first-b"]"#));
            siblings.insert(last.into(), json_fixture(r#"["last-a", "last-b"]"#));
            let owners = if object_owner {
                json!({"10":siblings})
            } else {
                json!([siblings])
            };
            let row = json!({"data":{"owners":owners}});
            let owner = if object_owner { "10" } else { "0" };
            assert_eq!(
                collection_pointer(&row, &routes),
                Some(format!(
                    "/data/owners/{owner}/{}",
                    crate::json_pointer::escape(last)
                )),
                "member order, not property spelling or reference-hop count: {row}"
            );
        }
    }
}
