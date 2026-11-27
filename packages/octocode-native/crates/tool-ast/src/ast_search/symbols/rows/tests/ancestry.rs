use super::rows;
use octocode_runtime_base::response::symbol_outline::declaration_rows;
use serde_json::Value;

#[test]
fn java_constructors_do_not_capture_sibling_methods() {
    for source in [
        "class Box {\n    Box() {}\n    void run() {}\n}\n",
        "class Box { Box() {} void run() {} }\n",
    ] {
        let flat = rows("Box.java", source, false)
            .iter()
            .map(|row| serde_json::from_str::<Value>(row).expect("row"))
            .collect::<Vec<_>>();
        let outline = declaration_rows(&flat);
        assert_eq!(outline.len(), 1, "{outline:?}");
        let members = outline[0]["members"].as_array().expect("class members");
        assert_eq!(members.len(), 2, "{outline:?}");
        assert_eq!(members[0]["symbolName"], "Box");
        assert_eq!(members[1]["symbolName"], "run");
        assert!(members[0].get("members").is_none(), "{outline:?}");
        assert!(
            outline
                .iter()
                .all(|row| !row.to_string().contains("_declarationIndex"))
        );
        assert!(
            outline
                .iter()
                .all(|row| !row.to_string().contains("_parentIndex"))
        );
    }
}

#[test]
fn same_line_same_named_containers_retain_their_own_members() {
    let flat = rows(
        "blocks.rs",
        "struct Box; impl Box { fn one() {} } impl Box { fn two() {} }\n",
        false,
    )
    .iter()
    .map(|row| serde_json::from_str::<Value>(row).expect("row"))
    .collect::<Vec<_>>();
    let outline = declaration_rows(&flat);
    assert_eq!(outline.len(), 3, "{outline:?}");
    assert_eq!(outline[1]["members"][0]["symbolName"], "one");
    assert_eq!(outline[2]["members"][0]["symbolName"], "two");
}
