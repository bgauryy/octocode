use crate::support;
use serde_json::json;
use support::Workspace;

fn search(root: &str, extra: serde_json::Value) -> serde_json::Value {
    let mut query = json!({"path":root,"matchString": ["needle"]});
    for (key, value) in extra.as_object().unwrap() {
        query[key] = value.clone();
    }
    query
}

#[tokio::test]
async fn cli_exit_is_two_when_any_batch_row_is_rejected() {
    let workspace = Workspace::new();
    let file = workspace.write("src/a.txt", "needle\n");
    let root = file.parent().unwrap().to_string_lossy().into_owned();
    let input =
        json!({"queries":[search(&root, json!({})), search(&root, json!({"serchText":"x"}))]});
    let output = workspace
        .cli()
        .args(["localSearch", &input.to_string(), "--json"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["results"][1]["data"]["errorCode"], "invalidInput");
    assert_ne!(value["results"][0]["status"], "error");
}

/// Public continuation inputs work across processes and return each file once.
#[test]
fn cli_pages_follow_only_the_cursor_across_processes() {
    let workspace = Workspace::new();
    let mut file = None;
    for index in 0..30 {
        file = Some(workspace.write(
            &format!("src/p{index:02}.txt"),
            "needle ".repeat(400) + "\n",
        ));
    }
    let root = file
        .unwrap()
        .parent()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let query = search(&root, json!({}));
    let mut input = json!({"queries":[query],"responseChars":4000});
    let mut files = Vec::new();
    let mut pages = 0;
    loop {
        pages += 1;
        assert!(pages < 100, "the walk terminates");
        let output = workspace
            .cli()
            .env("OCTOCODE_STORAGE_MODE", "persistent")
            .args(["localSearch", &input.to_string(), "--json"])
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        for row in value["results"].as_array().unwrap() {
            for file in row["data"]["files"].as_array().into_iter().flatten() {
                files.push(file["path"].as_str().unwrap().to_owned());
            }
        }
        let call = if value["responsePagination"]["hasMore"] == true {
            &value["responsePagination"]["next"]
        } else {
            &value["results"][0]["data"]["next"]["nextPage"]
        };
        if call.is_null() {
            break;
        }
        assert_eq!(call["kind"], "continue");
        assert_eq!(call["tool"], "localSearch");
        let next = &call["input"];
        assert_eq!(next.as_object().unwrap().len(), 1, "{next}");
        assert!(next["cursor"].is_string());
        input = next.clone();
    }
    assert!(pages > 1);
    files.sort();
    let expected: Vec<String> = (0..30).map(|index| format!("p{index:02}.txt")).collect();
    let names: Vec<String> = files
        .iter()
        .map(|path| path.rsplit('/').next().unwrap().to_owned())
        .collect();
    assert_eq!(names, expected);
}
