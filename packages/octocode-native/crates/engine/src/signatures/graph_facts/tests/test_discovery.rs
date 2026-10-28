use super::*;

#[cfg(feature = "tree-sitter-cpp")]
#[test]
fn googletest_declarations_require_framework_context_and_preserve_test_names() {
    let source = "#include <gtest/gtest.h>\nTEST_F(Suite, HandlesValue) { run(); }\nTEST_P(Matrix, HandlesVariant) {}\n";
    let value = facts(source, "tests.cpp");
    let rows = value["declarations"].as_array().unwrap();
    let first = rows
        .iter()
        .find(|row| row["name"] == "Suite.HandlesValue")
        .expect("named test");
    assert_eq!(
        first["testIdentity"],
        serde_json::json!({"framework":"googletest","suite":"Suite","name":"HandlesValue","parameterized":false,"kind":"declaration"})
    );
    let span = &first["selectionByteRange"];
    let start = span["range"]["start_byte"].as_u64().unwrap() as usize;
    let end = span["range"]["end_byte"].as_u64().unwrap() as usize;
    assert_eq!(&source[start..end], "HandlesValue");
    assert_eq!(span["unit"], 0);
    assert_eq!(first["parseAssessment"]["status"], "parsed");
    let second = rows
        .iter()
        .find(|row| row["name"] == "Matrix.HandlesVariant")
        .expect("parameterized");
    assert_eq!(second["testIdentity"]["parameterized"], true);
    assert_eq!(value["calls"][0]["caller"], "Suite.HandlesValue");
}

#[cfg(feature = "tree-sitter-cpp")]
#[test]
fn lookalike_functions_custom_macros_and_unobserved_frameworks_are_not_tests() {
    for source in [
        "TEST_F(Suite, Name) {}\n",
        "#include <gtest/gtest.h>\nvoid TEST_F(int Suite, int Name) {}\n",
        "#include <gtest/gtest.h>\n#define TEST_F(A,B) other(A,B)\nTEST_F(Suite, Name) {}\n",
        "#include <gtest/gtest.h>\n#undef TEST_F\nTEST_F(Suite, Name) {}\n",
        "#include <gtest/gtest.h>\n#undef TEST_F /* removed locally */\nTEST_F(Suite, Name) {}\n",
        "#include <gtest/gtest.h>\n#undef /* removed */ TEST_F/* local */\nTEST_F(Suite, Name) {}\n",
        "#include <gtest/gtest.h>\n#  undef TEST_F\nTEST_F(Suite, Name) {}\n",
        "#include <gtest/gtest.h>\nTEST_F(Suite, computed(Name)) {}\n",
        "#include <gtest/gtest.h>\nvoid custom::TEST_F(Suite, Name) {}\n",
    ] {
        let value = facts(source, "lookalike.cpp");
        assert!(
            value["declarations"]
                .as_array()
                .unwrap()
                .iter()
                .all(|d| d.get("testIdentity").is_none()),
            "{source}: {value}"
        );
    }
}

#[test]
fn rust_test_attribute_is_distinct_from_similarly_named_attributes_and_helpers() {
    let value = facts(
        "#[test]\nfn handles_value() {}\n#[test_case]\nfn generated() {}\nfn test_helper() {}\n#[other::test]\nfn other_runner() {}\n",
        "tests.rs",
    );
    let rows = value["declarations"].as_array().unwrap();
    let identified: Vec<_> = rows
        .iter()
        .filter(|d| d.get("testIdentity").is_some())
        .collect();
    assert_eq!(identified.len(), 1, "{value}");
    assert_eq!(
        identified[0]["testIdentity"],
        serde_json::json!({"framework":"rust","name":"handles_value","parameterized":false,"kind":"declaration"})
    );
}
