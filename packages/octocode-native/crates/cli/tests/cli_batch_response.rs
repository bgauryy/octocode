use crate::support;
use serde_json::json;
use support::Workspace;

fn search(root: &str, extra: serde_json::Value) -> serde_json::Value {
    let mut query =
        json!({"path":root,"matchString":"needle","mainGoal": "test", "reasoning":"Find needles."});
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

#[test]
fn cli_exit_is_zero_while_response_pages_remain() {
    let workspace = Workspace::new();
    // 30 files of 2,000-character hits overflow the constant 20,000-character
    // automatic page.
    let mut file = None;
    for index in 0..30 {
        file = Some(workspace.write(&format!("src/p{index}.txt"), "needle ".repeat(400) + "\n"));
    }
    let root = file
        .unwrap()
        .parent()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let query = search(&root, json!({"pageSize":30,"matchContentLength":2000}));
    let output = workspace
        .cli()
        .args([
            "localSearch",
            &json!({"queries":[query]}).to_string(),
            "--json",
        ])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["responsePagination"]["hasMore"], true, "{value}");
    assert_eq!(output.status.code(), Some(0));
}

/// Every response page is reached across CLI processes by sending only the
/// previous page's cursor (`{"responseSnapshot": …}`), each file exactly
/// once, under one `totalPages`.
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
    let query = search(&root, json!({"pageSize":30,"matchContentLength":2000}));
    let mut input = json!({"queries":[query]});
    let mut files = Vec::new();
    let mut total = None;
    for page in 1.. {
        assert!(page < 100, "the walk terminates");
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
        let pagination = &value["responsePagination"];
        assert_eq!(pagination["currentPage"], page, "{pagination}");
        let pages = pagination["totalPages"].clone();
        assert_eq!(*total.get_or_insert(pages.clone()), pages, "{pagination}");
        if pagination["hasMore"] != true {
            break;
        }
        let next = &pagination["next"]["query"];
        assert_eq!(next.as_object().unwrap().len(), 1, "{next}");
        input = json!({"responseSnapshot": next["responseSnapshot"].clone()});
    }
    files.sort();
    let expected: Vec<String> = (0..30).map(|index| format!("p{index:02}.txt")).collect();
    let names: Vec<String> = files
        .iter()
        .map(|path| path.rsplit('/').next().unwrap().to_owned())
        .collect();
    assert_eq!(names, expected);
}
