//! A partial reason says what it means when it fires, once per response; a
//! response whose rows hold no reason carries none of that text.
use crate::support::{Run as _, Workspace};
use serde_json::{Value, json};

fn warnings_of(output: &Value) -> Vec<String> {
    output["results"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|row| {
            ["warnings", "hints"].into_iter().flat_map(|key| {
                row["data"][key]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
            })
        })
        .filter_map(|text| text.as_str().map(str::to_owned))
        .collect()
}

#[tokio::test]
async fn a_reason_is_explained_once_when_it_fires_and_never_otherwise() {
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
    let said = warnings_of(&fired)
        .into_iter()
        .filter(|text| text.starts_with("parseRecovered: "))
        .collect::<Vec<_>>();
    assert_eq!(said.len(), 1, "once per response: {fired}");
    assert!(said[0].contains("recovered from syntax errors"), "{said:?}");

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
    assert!(
        !clean.to_string().contains("recovered from syntax errors"),
        "{clean}"
    );
    runtime.close().await;
}
