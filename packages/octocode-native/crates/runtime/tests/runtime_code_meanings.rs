//! A partial reason reaches a caller as its code alone, on every row that
//! carries it: what the code means is said once, by the output reference
//! (`schema <tool> --view output`), never in a response.
use crate::support::checks::assert_no_reason_meaning;
use crate::support::{Run as _, Workspace};
use serde_json::json;

#[tokio::test]
async fn a_fired_reason_is_its_code_and_no_response_explains_it() {
    let workspace = Workspace::new();
    workspace.write("broken/a.ts", "function ok() { return 1 }\nclass {{{ (\n");
    workspace.write("broken/b.ts", "function ok2() { return 2 }\nclass {{{ (\n");
    workspace.write("clean/c.ts", "function ok3() { return 3 }\n");
    let runtime = workspace.runtime(&[]);
    let row = |path: &str| json!({"path":path,"parser":"typescript","operation":"symbols"});
    let fired = runtime
        .run(
            "fired".into(),
            "astSearch".into(),
            json!({"queries":[row("broken/a.ts"),row("broken/b.ts")]}),
        )
        .await
        .expect("rows")
        .structured_content;
    for result in fired["results"].as_array().expect("rows") {
        assert_eq!(
            result["data"]["partialReasons"],
            json!(["parseRecovered"]),
            "{result}"
        );
    }
    assert_no_reason_meaning(&fired);

    let clean = runtime
        .run(
            "clean".into(),
            "astSearch".into(),
            json!({"queries":[row("clean/c.ts")]}),
        )
        .await
        .expect("row")
        .structured_content;
    assert!(clean["results"][0]["data"].get("partialReasons").is_none());
    assert_no_reason_meaning(&clean);
    runtime.close().await;
}
