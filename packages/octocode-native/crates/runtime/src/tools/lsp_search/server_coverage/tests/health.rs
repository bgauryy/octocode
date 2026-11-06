//! A server's report about one project marks only rows that touch it.
use super::*;
use octocode_engine::lsp::client::HealthLevel;
use octocode_engine::lsp::client::ServerHealth;

fn query() -> LspSearchQuery {
    serde_json::from_value(json!({
        "operation": "subtypes", "path": "/r/guava/src/F.java", "symbolName": "F", "line": 1
    }))
    .expect("query")
}

fn failed_import(project: Option<&str>) -> Option<ServerHealth> {
    Some(ServerHealth {
        level: HealthLevel::Error,
        message: Some("The server could not import the project at /r/gradle/".into()),
        project: project.map(str::to_owned),
    })
}

/// guava: the Maven projects imported, `integration-tests/gradle` did not. A row
/// anchored in the Maven root that lists nothing there stays complete; a row
/// whose anchor or a listed result lies in the failed project is partial, as
/// is every row when the report is about the whole server.
#[test]
fn a_project_import_failure_marks_only_rows_that_touch_that_project() {
    let scope = Scope::new("/r".into(), Vec::new());
    let maven_row = || {
        json!({"status":"success","matches":[{"path":"guava/src/G.java"}],
            "related":[{"kind":"related","tool":"localSearch","input":{"queries":[{"path":"gradle"}]}}]})
    };
    let mut untouched = maven_row();
    flag_server_health(
        &mut untouched,
        &query(),
        failed_import(Some("/r/gradle/")),
        "/r/guava/src/F.java",
        &scope,
    );
    assert!(untouched.get("isPartial").is_none(), "{untouched}");
    assert!(untouched.get("warnings").is_none(), "{untouched}");

    let mut listed = json!({"status":"success","matches":[{"path":"gradle/src/H.java"}]});
    flag_server_health(
        &mut listed,
        &query(),
        failed_import(Some("/r/gradle/")),
        "/r/guava/src/F.java",
        &scope,
    );
    assert_eq!(listed["isPartial"], true, "{listed}");

    let mut by_uri = json!({"status":"success","items":[{"uri":"file:///r/gradle/src/H.java"}]});
    flag_server_health(
        &mut by_uri,
        &query(),
        failed_import(Some("/r/gradle")),
        "/r/guava/src/F.java",
        &scope,
    );
    assert_eq!(by_uri["isPartial"], true, "{by_uri}");

    let mut anchored = json!({"status":"empty"});
    flag_server_health(
        &mut anchored,
        &query(),
        failed_import(Some("/r/gradle/")),
        "/r/gradle/src/H.java",
        &scope,
    );
    assert_eq!(anchored["isPartial"], true, "{anchored}");

    let mut whole_server = maven_row();
    flag_server_health(
        &mut whole_server,
        &query(),
        failed_import(None),
        "/r/guava/src/F.java",
        &scope,
    );
    assert_eq!(whole_server["isPartial"], true, "{whole_server}");
}
