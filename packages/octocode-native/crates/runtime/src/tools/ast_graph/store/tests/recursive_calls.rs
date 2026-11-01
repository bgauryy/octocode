use super::*;

#[test]
fn recursive_and_same_file_calls_remain_visible_without_reexpanding_seeds() {
    let dir = git_app(&[(
        "calls.ts",
        "export function alpha(n: number): number {\n  return n > 0 ? alpha(n - 1) + beta(n - 1) : 0;\n}\nexport function beta(n: number): number {\n  return alpha(n);\n}\n",
    )]);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let expected = vec!["calls.ts#alpha".to_owned(), "calls.ts#beta".to_owned()];
    for target in ["calls.ts#alpha", "calls.ts"] {
        for operation in ["callers", "callees"] {
            for depth in [1, 20] {
                let answer = ask(root, operation, Some(target), |o| o.depth = Some(depth));
                let mut actual = ids(&answer);
                actual.sort();
                assert_eq!(actual, expected, "{operation} {target}: {}", answer.value);
                assert!(
                    answer.value["rows"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|row| row["depth"] == 1)
                );
            }
        }
    }
    let mut paged = Vec::new();
    for offset in 0..2 {
        let page = ask(root, "callees", Some("calls.ts#alpha"), |o| {
            o.limit = Some(1);
            o.offset = Some(offset);
        });
        assert_eq!(page.value["pagination"]["totalItems"], 2);
        paged.extend(ids(&page));
    }
    paged.sort();
    assert_eq!(paged, expected);
}
