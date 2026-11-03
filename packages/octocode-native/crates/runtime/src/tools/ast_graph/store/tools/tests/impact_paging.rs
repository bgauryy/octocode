use super::*;

#[test]
fn impact_retains_every_test_function_for_shared_response_pagination() {
    let dir = Fixture::new();
    let count = 125;
    let tests = (0..count)
        .map(|index| format!("#[test] fn case{index:03}() {{ target(); }}\n"))
        .collect::<String>();
    dir.write(
        "app/src/lib.rs",
        format!("pub fn target() {{}}\n#[cfg(test)] mod tests {{ use super::*; {tests} }}\n"),
    );
    dir.write(
        "app/Cargo.toml",
        "[package]\nname='impact'\nversion='0.1.0'\n",
    );
    let built = call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    let impact = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"impact","path":"app","graph":built["id"],"target":"src/lib.rs#target","limit":1}),
    );
    let expected = (0..count)
        .map(|index| json!(format!("src/lib.rs#tests.case{index:03}")))
        .collect::<Vec<_>>();
    assert_eq!(impact["summary"]["testFunctions"], json!(expected));
    assert_eq!(impact["summary"]["testsToRun"], json!(["src/lib.rs"]));
    assert_eq!(impact["rows"].as_array().expect("impact rows").len(), 1);
    assert!(impact.get("next").is_none(), "{impact}");
}
