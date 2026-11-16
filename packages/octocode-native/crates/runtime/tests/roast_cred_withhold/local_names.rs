//! Credential file names the policy withholds from a local read.
use crate::support::{Workspace, call, row_data, row_status};
use serde_json::json;

#[tokio::test]
async fn cargo_and_terraform_credential_files_are_withheld() {
    let workspace = Workspace::new();
    workspace.write(
        ".cargo/credentials.toml",
        "[registry]\ntoken = \"plainsecretvalue\"\n",
    );
    workspace.write(
        ".terraform.d/credentials.tfrc.json",
        "{\"note\":\"plainsecretvalue\"}",
    );
    let runtime = workspace.runtime(&[]);
    for name in [
        ".cargo/credentials.toml",
        ".terraform.d/credentials.tfrc.json",
    ] {
        let outcome = call(&runtime, "localFetch", json!({"path": name}))
            .await
            .expect("read");
        let text = outcome.structured_content.to_string();
        assert!(!text.contains("plainsecretvalue"), "{name}: {text}");
        assert_eq!(
            row_data(&outcome)["errorCode"],
            "pathPolicyDenied",
            "{name}: {text}"
        );
        let _ = row_status(&outcome);
    }
    runtime.close().await;
}
