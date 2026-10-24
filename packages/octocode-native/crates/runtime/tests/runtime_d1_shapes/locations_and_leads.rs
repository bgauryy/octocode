//! Location rows named like tool inputs, runnable recovery leads, and one camelCase error-code set.

use crate::support::{self, Run as _, Workspace};
use serde_json::json;

// ---------- location rows are objects named like tool inputs ----------

#[tokio::test]
async fn location_rows_are_objects_named_like_tool_inputs() {
    let workspace = Workspace::new();
    workspace.write(
        "pkg/src/lib.rs",
        "pub fn run() {\n    let a = 1;\n    let b = a + 1;\n}\n",
    );
    workspace.write("pkg/a.txt", "x\n");
    let runtime = workspace.runtime(&[]);
    // The enclosing declaration is an object an lspSearch/localFetch
    // call takes as is.
    let hits = support::run(
        &runtime,
        "localSearch",
        json!({"path":"pkg","matchString":["let"]}),
    )
    .await;
    let first = &hits.data()["files"][0]["matches"][0];
    assert_eq!(
        first["enclosing"],
        json!({"symbolName":"run","kind":"function","line":1,"endLine":4}),
        "{}",
        hits.structured_content
    );
    assert!(first.get("in").is_none(), "{first}");
    // A declaration without members is an object too.
    let outline = support::run(
        &runtime,
        "astSearch",
        json!({"operation":"symbols","path":"pkg"}),
    )
    .await;
    assert_eq!(
        outline.data()["files"][0]["symbols"][0],
        json!({"symbolName":"run","kind":"function","line":1,"endLine":4,"exported":true}),
        "{}",
        outline.structured_content
    );
    // Match rows are objects with their 1-based column: two matches can
    // start on one line.
    let matched = support::run(
        &runtime,
        "astSearch",
        json!({"operation":"match","path":"pkg","parser":"rust","rule":{"pattern":"let $A = $B;"}}),
    )
    .await;
    assert_eq!(
        matched.data()["files"][0]["matches"][0],
        json!({"line":2,"column":5,"value":"let a = 1;"}),
        "{}",
        matched.structured_content
    );
    runtime.close().await;
}

// ---------- lead names and runnable recoveries ----------

#[tokio::test]
async fn a_case_sensitive_miss_leads_to_a_runnable_ignore_case_read() {
    let workspace = Workspace::new();
    workspace.write("pkg/a.txt", "alpha\nHello World\n");
    let runtime = workspace.runtime(&[]);
    let miss = support::run(
        &runtime,
        "localFetch",
        json!({"path":"pkg/a.txt","matchString":["hello World"]}),
    )
    .await;
    let ignore = miss.data()["related"]
        .as_array()
        .expect("related calls")
        .iter()
        .find(|call| {
            call["tool"] == "localFetch" && call["input"]["queries"][0]["caseMode"] == "insensitive"
        })
        .expect("explicit insensitive alternative");
    assert_eq!(ignore["kind"], "related");
    let hit = runtime
        .run(
            "case-recovery".into(),
            "localFetch".into(),
            ignore["input"].clone(),
        )
        .await
        .expect("runnable related read");
    assert!(
        support::row_data(&hit)["content"]
            .as_str()
            .is_some_and(|text| text.contains("Hello World")),
        "{}",
        hit.structured_content
    );
    // A miss that already ignored case offers no ignoreCase lead.
    let insensitive = support::run(
        &runtime,
        "localFetch",
        json!({"path":"pkg/a.txt","matchString":["absent"],"caseMode":"insensitive"}),
    )
    .await;
    assert!(
        insensitive.data()["related"]
            .as_array()
            .into_iter()
            .flatten()
            .all(|call| call["tool"] != "localFetch"
                || call["input"]["queries"][0]["caseMode"] != "insensitive")
    );
    runtime.close().await;
}

#[tokio::test]
async fn a_missing_file_leads_to_find_file() {
    let workspace = Workspace::new();
    workspace.write("pkg/server.rs", "fn main() {}\n");
    let runtime = workspace.runtime(&[]);
    let missing = support::run(&runtime, "localFetch", json!({"path":"pkg/server.ts"})).await;
    let find = missing.data()["related"]
        .as_array()
        .expect("related calls")
        .iter()
        .find(|call| call["tool"] == "structureSearch")
        .expect("file discovery");
    assert_eq!(find["kind"], "related");
    let listing = runtime
        .run(
            "find-file".into(),
            "structureSearch".into(),
            find["input"].clone(),
        )
        .await
        .expect("runnable recovery");
    assert!(listing.structured_content.to_string().contains("server.rs"));
    runtime.close().await;
}

// ---------- one camelCase error-code set ----------

#[tokio::test]
async fn error_codes_are_declared_camel_case_names() {
    let workspace = Workspace::new();
    workspace.write("pkg/a.rs", "fn a() {}\n");
    let runtime = workspace.runtime(&[]);
    let unsupported = runtime.run("invalid-parser".into(),"astSearch".into(),json!({"queries":[{"operation":"match","path":"pkg/a.rs","parser":"cobol","rule":{"pattern":"fn $A() {}"}}]})).await.expect_err("unknown parser rejected at admission");
    assert_eq!(
        unsupported.code,
        octocode_contract_types::ErrorCode::InvalidInput
    );
    let missing = support::run(&runtime, "localFetch", json!({"path":"pkg/none.rs"})).await;
    assert_eq!(missing.data()["errorCode"], "pathNotFound");
    {
        let code = support::row_data(&missing)["errorCode"]
            .as_str()
            .expect("code");
        assert!(
            code.chars().all(|c| c.is_ascii_alphanumeric()),
            "{code} is not camelCase"
        );
        assert!(
            octocode_native::id::error_codes::parse(code).is_some(),
            "{code} is not declared"
        );
    }
    runtime.close().await;
}
