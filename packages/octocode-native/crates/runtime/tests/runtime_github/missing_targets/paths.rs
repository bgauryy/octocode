use crate::support::{Workspace, call, row_data, row_status};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A wrong-case path recovers to the case-corrected file, and a missing
/// file to its nearest existing directory, never to another missing path.
#[tokio::test]
async fn gh_file_read_of_a_missing_path_recovers_to_what_exists() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;
    let listing = |entries: serde_json::Value| ResponseTemplate::new(200).set_body_json(entries);
    for (dir, entries) in [
        (
            "",
            json!([
                {"name":"tokio","path":"tokio","type":"dir"},
                {"name":"README.md","path":"README.md","type":"file","size":3,"sha":"1"}
            ]),
        ),
        (
            "/tokio",
            json!([{"name":"src","path":"tokio/src","type":"dir"}]),
        ),
        (
            "/tokio%2Fsrc",
            json!([
                {"name":"lib.rs","path":"tokio/src/lib.rs","type":"file","size":3,"sha":"2"}
            ]),
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents{dir}")))
            .respond_with(listing(entries))
            .mount(&server)
            .await;
    }
    let not_found = || ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"}));
    for missing in [
        "Tokio%2Fsrc%2Flib.rs",
        "Tokio%2Fsrc",
        "Tokio",
        "tokio%2Fsrc%2Fnope.rs",
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/{missing}")))
            .respond_with(not_found())
            .mount(&server)
            .await;
    }
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let read =
        |file: &str| json!({"owner":"a","repo":"b","path":file,"ref":"main","forceRefresh":true});
    let outcome = call(&runtime, "ghGetFileContent", read("Tokio/src/lib.rs"))
        .await
        .expect("error row");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "notFound", "{data}");
    assert_eq!(
        crate::support::checks::related_call(data, "ghGetFileContent", |_| true)["input"]["queries"]
            [0]["path"],
        "tokio/src/lib.rs",
        "{data}"
    );
    // Recovery leads pin the commit the read resolved, not the movable ref.
    assert_eq!(
        crate::support::checks::related_call(data, "ghGetFileContent", |_| true)["input"]["queries"]
            [0]["ref"],
        sha,
        "{data}"
    );
    // The case-fixed read is the one repair; no listing beside it.
    assert!(
        !data["related"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|call| call["tool"] == "ghStructure"
                && (call["input"]["queries"][0]["operation"] != "refs")),
        "{data}"
    );
    let outcome = call(&runtime, "ghGetFileContent", read("tokio/src/nope.rs"))
        .await
        .expect("error row");
    let data = row_data(&outcome);
    assert!(
        !data["related"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|call| call["tool"] == "ghGetFileContent" && (true)),
        "{data}"
    );
    assert_eq!(
        crate::support::checks::related_call(
            data,
            "ghStructure",
            |input| input["queries"][0]["operation"] != "refs"
        )["input"]["queries"][0]["path"],
        "tokio/src",
        "{data}"
    );
    // A hint is not proof, so no continuation carries confidence:"exact".
    assert!(
        crate::support::checks::related_call(
            data,
            "ghStructure",
            |input| input["queries"][0]["operation"] != "refs"
        )
        .get("confidence")
        .is_none(),
        "{data}"
    );
    runtime.close().await;
}

/// A ghStructure listing of a missing path recovers like
/// ghGetFileContent: `next.viewTree` lists the nearest existing directory
/// (case-corrected), never another missing path.
#[tokio::test]
async fn gh_structure_of_a_missing_path_recovers_to_the_nearest_directory() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;
    let listing = |entries: serde_json::Value| ResponseTemplate::new(200).set_body_json(entries);
    for (dir, entries) in [
        ("", json!([{"name":"src","path":"src","type":"dir"}])),
        (
            "/src",
            json!([{"name":"Tools","path":"src/Tools","type":"dir"}]),
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents{dir}")))
            .respond_with(listing(entries))
            .mount(&server)
            .await;
    }
    let not_found = || ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"}));
    for missing in [
        "src%2Fnope",
        "src%2Ftools",
        "no",
        "no%2Fsuch",
        "no%2Fsuch%2Fdir",
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/{missing}")))
            .respond_with(not_found())
            .mount(&server)
            .await;
    }
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let tree =
        |dir: &str| json!({"operation":"tree","owner":"a","repo":"b","path":dir,"ref":"main"});
    for (requested, nearest) in [
        ("src/nope", "src"),
        ("src/tools", "src/Tools"),
        ("no/such/dir", "."),
    ] {
        let outcome = call(&runtime, "ghStructure", tree(requested))
            .await
            .expect("error row, not a contract violation");
        assert_eq!(
            row_status(&outcome),
            "error",
            "{}",
            outcome.structured_content
        );
        let data = row_data(&outcome);
        assert_eq!(data["errorCode"], "notFound", "{data}");
        assert_eq!(
            crate::support::checks::related_call(data, "ghStructure", |input| input["queries"][0]
                ["operation"]
                != "refs")["tool"],
            "ghStructure",
            "{data}"
        );
        assert_eq!(
            crate::support::checks::related_call(data, "ghStructure", |input| input["queries"][0]
                ["operation"]
                != "refs")["input"]["queries"][0]["path"],
            nearest,
            "{data}"
        );
        // Recovery leads pin the commit the listing resolved.
        assert_eq!(
            crate::support::checks::related_call(data, "ghStructure", |input| input["queries"][0]
                ["operation"]
                != "refs")["input"]["queries"][0]["ref"],
            sha,
            "{data}"
        );
        // A hint is not proof, so no continuation carries confidence:"exact".
        assert!(
            crate::support::checks::related_call(data, "ghStructure", |input| input["queries"][0]
                ["operation"]
                != "refs")
            .get("confidence")
            .is_none(),
            "{data}"
        );
        assert!(
            data["error"]
                .as_str()
                .unwrap_or_default()
                .contains(requested),
            "{data}"
        );
    }
    runtime.close().await;
}
