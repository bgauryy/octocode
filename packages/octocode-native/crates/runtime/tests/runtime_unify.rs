//! One redaction mechanism for every tool, read from the serialized result
//! an agent receives: a replaced span is flagged `redacted: true` and nothing
//! else says so, a bare value after a label is a value only in a config
//! file, and a shown name that several entries share is not a missing path.
//! All secrets are fake.

use crate::support::{Run as _, Workspace, call, query_path, row_data};
use octocode_test_support::json_fixture;
use serde_json::json;

const TOKEN: &str = "ghp_aB3dE5gH7jK9mN1pQ3sT5vW7yZ9bD1fH3jK5";
const OTHER: &str = "ghp_zY9xW7vU5tS3rQ1pN9mL7kJ5hG3fD1bA9zX7";

fn rendered(outcome: &octocode_runtime_base::response::outcome::ToolOutcome) -> String {
    outcome.structured_content.to_string()
}

fn flagged(outcome: &octocode_runtime_base::response::outcome::ToolOutcome) -> bool {
    row_data(outcome)["redacted"] == json!(true)
}

#[tokio::test]
async fn a_window_over_a_key_block_carries_the_flag_and_no_notice() {
    let workspace = Workspace::new();
    let file = workspace.write(
        "k.ts",
        "export const pem = `-----BEGIN RSA PRIVATE KEY-----\nMIIEpQIBAAKCAQEAFAKEfakeFAKEfakeFAKEfakeFAKEfakeFAKEfakeFAKEfake\nQ2xhdWRlRmFrZUtleUJvZHlMaW5lVHdvRmFrZUZha2VGYWtlRmFrZUZha2VGYWtl\n-----END RSA PRIVATE KEY-----`;\nexport const after = 1;\n",
    );
    let runtime = workspace.runtime(&[]);
    let read = call(&runtime, "localFetch", query_path(&file, json!({})))
        .await
        .expect("localFetch");
    let text = rendered(&read);
    assert!(flagged(&read), "{text}");
    assert!(!text.contains("Q2xhdWRl"), "{text}");
    assert!(
        !text.to_lowercase().contains("redacted private-key"),
        "{text}"
    );
    assert!(row_data(&read).get("warnings").is_none(), "{text}");
    runtime.close().await;
}

#[tokio::test]
async fn a_line_past_the_streamed_bound_flags_its_replacement() {
    let workspace = Workspace::new();
    let filler = "x".repeat(5 * 1024 * 1024 + 300 * 1024);
    let line = format!("{filler} markerone {TOKEN} markertwo {filler}\n");
    let file = workspace.write("long.txt", line);
    let runtime = workspace.runtime(&[]);
    let found = call(
        &runtime,
        "localSearch",
        query_path(&file, json_fixture(r#"{"matchString": ["markerone"]}"#)),
    )
    .await
    .expect("localSearch");
    let text = rendered(&found);
    assert!(text.contains("markerone"), "the hit is found");
    assert!(!text.contains(TOKEN), "{}", &text[..text.len().min(400)]);
    assert!(text.contains("[REDACTED-"), "the secret's span is replaced");
    assert!(
        flagged(&found),
        "a replaced span is flagged: {}",
        &text[..text.len().min(400)]
    );
    runtime.close().await;
}

#[tokio::test]
async fn an_ambiguous_shown_name_is_named_by_localfetch_as_by_the_other_tools() {
    let workspace = Workspace::new();
    workspace.write(&format!("deploy-{TOKEN}.txt"), "one\n");
    workspace.write(&format!("deploy-{OTHER}.txt"), "two\n");
    let shown = workspace
        .workspace
        .join("deploy-[REDACTED-GITHUBTOKENS].txt");
    let runtime = workspace.runtime(&[]);
    let read = call(&runtime, "localFetch", query_path(&shown, json!({})))
        .await
        .expect("localFetch");
    let text = rendered(&read);
    assert!(text.contains("several entries show that name"), "{text}");
    assert!(!text.contains("Path does not exist"), "{text}");
    runtime.close().await;
}

#[tokio::test]
async fn a_truncated_key_body_keeps_its_ellipsis() {
    let workspace = Workspace::new();
    let body = "MIIEpAIBAAKCAQEAazSbx/LFFsNj0jn5+XC2fxG1hsULQ7Fbo6ACNNDSWAdMYqYy8zqWNA5vY9raTQXY6IYJnKGTko2W6YBMpEuGgNud";
    let file = workspace.write(
        "doc.md",
        format!("Your key file starts like this:\n\n    -----BEGIN RSA PRIVATE KEY-----\n    {body}...\n    ...\n\n## Downloads\n"),
    );
    let runtime = workspace.runtime(&[]);
    let read = call(&runtime, "localFetch", query_path(&file, json!({})))
        .await
        .expect("localFetch");
    let content = row_data(&read)["content"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(!content.contains(body), "{content}");
    assert!(
        content
            .lines()
            .any(|line| line.contains("[REDACTED") && line.trim_end().ends_with("...")),
        "{content}"
    );
    runtime.close().await;
}

/// A bare value after a label is a value in a config file and an identifier
/// in code; a quoted value is a value in both.
#[tokio::test]
async fn a_bare_value_is_redacted_by_file_kind_and_a_quoted_one_everywhere() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    for name in [
        "app.properties",
        "app.ini",
        "app.yaml",
        "app.yml",
        "app.toml",
        "app.conf",
        "app.cfg",
    ] {
        let file = workspace.write(name, "password=letmein1\nname=visible\n");
        let read = call(&runtime, "localFetch", query_path(&file, json!({})))
            .await
            .expect("localFetch");
        let text = rendered(&read);
        assert!(!text.contains("letmein1"), "{name}: {text}");
        assert!(
            text.contains("password=") && text.contains("name=visible"),
            "{name}: {text}"
        );
        assert!(flagged(&read), "{name}: {text}");
    }
    for (name, source) in [
        ("app.py", "password = userPass1\n"),
        ("app.ts", "const x = { password = userPass1 };\n"),
        ("app.rs", "let password = userPass1;\n"),
    ] {
        let file = workspace.write(name, source);
        let read = call(&runtime, "localFetch", query_path(&file, json!({})))
            .await
            .expect("localFetch");
        let text = rendered(&read);
        assert!(text.contains("userPass1"), "{name}: {text}");
        assert!(!flagged(&read), "{name}: {text}");
    }
    for name in ["q.py", "q.ts", "q.yaml", "q.properties"] {
        let file = workspace.write(name, "password = \"userPass1\"\n");
        let read = call(&runtime, "localFetch", query_path(&file, json!({})))
            .await
            .expect("localFetch");
        let text = rendered(&read);
        assert!(!text.contains("userPass1"), "{name}: {text}");
        assert!(flagged(&read), "{name}: {text}");
    }
    runtime.close().await;
}

/// A saved response page names a file by its shown name; the true name of a
/// file named after a credential reaches no file of the home, and the saved
/// pages still run to the end.
#[tokio::test]
async fn saved_pages_keep_the_shown_name_of_a_credential_named_file() {
    let workspace = Workspace::new();
    let lines: String = (0..200).map(|i| format!("needle {i}\n")).collect();
    let file = workspace.write(&format!("deploy-{TOKEN}.txt"), lines);
    let env = [("OCTOCODE_STORAGE_MODE", "persistent".into())];
    let runtime = workspace.runtime(&env);
    let mut output = runtime
        .run(
            "first".into(),
            "localSearch".into(),
            json!({"queries":[{"path":file,"matchString":["needle"]}],"responseChars":800}),
        )
        .await
        .expect("first page")
        .structured_content;
    let mut pages = 1;
    while let Some(next) = crate::support::next_input(&output) {
        output = runtime
            .run(format!("page-{pages}"), "localSearch".into(), next)
            .await
            .expect("a saved page resumes")
            .structured_content;
        pages += 1;
        assert!(pages < 200, "the pages end");
    }
    assert!(pages > 1, "the response paged");
    let mut stack = vec![workspace.home.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("home").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(bytes) = std::fs::read(&path) {
                assert!(
                    !bytes
                        .windows(TOKEN.len())
                        .any(|window| window == TOKEN.as_bytes()),
                    "{} holds the true name",
                    path.display()
                );
            }
        }
    }
    runtime.close().await;
}

/// A call target the build replaced is flagged where the graph shows it.
#[tokio::test]
async fn a_graph_row_that_shows_a_replaced_target_is_flagged() {
    let workspace = Workspace::new();
    let root = workspace.workspace.clone();
    workspace.write(
        "computed.ts",
        format!("const client: any = {{}};\nexport function callIt(): void {{\n  client[\"{TOKEN}\"]();\n}}\n"),
    );
    let runtime = workspace.runtime(&[]);
    let built = call(&runtime, "graphBuild", json!({"path": root}))
        .await
        .expect("graphBuild");
    assert!(
        row_data(&built)["errorCode"].is_null(),
        "{}",
        rendered(&built)
    );
    let rows = call(
        &runtime,
        "graphQuery",
        json!({"operation":"unresolved","path":root,"target":"computed.ts"}),
    )
    .await
    .expect("graphQuery");
    let text = rendered(&rows);
    assert!(!text.contains(TOKEN), "{text}");
    assert!(
        text.contains("[REDACTED-"),
        "the target shows replaced: {text}"
    );
    assert!(flagged(&rows), "{text}");
    runtime.close().await;
}
