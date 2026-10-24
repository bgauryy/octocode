//! Path sandbox and security policy: denials outside allowed roots and withheld credential files.

use crate::support::{Workspace, call, row_data, row_status};
use serde_json::json;

#[tokio::test]
async fn local_fetch_denial_echoes_the_relative_request_not_an_expanded_prefix() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let outcome = call(
        &runtime,
        "localFetch",
        json!({"path": "../../octocode-outside-probe.txt"}),
    )
    .await
    .expect("typed denial row");
    assert_eq!(row_status(&outcome), "error");
    let error = row_data(&outcome)["error"]
        .as_str()
        .expect("error")
        .to_owned();
    let denied_path = error.split(" is outside").next().unwrap_or_default();
    assert_eq!(
        denied_path, "Path '../../octocode-outside-probe.txt'",
        "{error}"
    );
    let crate_dir = env!("CARGO_MANIFEST_DIR");
    let parent = std::path::Path::new(crate_dir)
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crate grandparent");
    assert!(!denied_path.contains(&*parent.to_string_lossy()), "{error}");
    runtime.close().await;
}

#[tokio::test]
async fn lsp_search_rejects_files_outside_allowed_roots_before_server_discovery() {
    let workspace = Workspace::new();
    let outside = workspace.write_outside_allowed_roots("secret.rs", "pub fn secret() {}\n");
    let runtime = workspace.runtime(&[]);
    let outcome = call(
        &runtime,
        "lspSearch",
        json!({
            "operation": "documentSymbols",
            "path": outside
        }),
    )
    .await
    .expect("typed lsp denial");
    assert_eq!(row_status(&outcome), "error");
    let rendered = serde_json::to_string(row_data(&outcome)).expect("json");
    assert!(
        rendered.contains("outside allowed directories"),
        "expected path-policy denial, got {rendered}"
    );
    assert_eq!(row_data(&outcome)["errorCode"], "outsideAllowedRoots");
    runtime.close().await;
}

/// Every local tool reports a sandbox refusal under one dedicated code, and
/// the recovery hint is keyed on that code.
#[tokio::test]
async fn local_tools_share_the_path_outside_allowed_roots_code() {
    let workspace = Workspace::new();
    let outside = workspace.write_outside_allowed_roots("denied.rs", "pub fn denied() {}\n");
    let runtime = workspace.runtime(&[]);
    for (tool, query) in [
        ("localFetch", json!({"path": outside})),
        (
            "localSearch",
            json!({"path": outside, "matchString":["denied"]}),
        ),
    ] {
        let outcome = call(&runtime, tool, query).await.expect("typed denial");
        assert_eq!(row_status(&outcome), "error", "{tool}");
        let data = row_data(&outcome);
        assert_eq!(data["errorCode"], "outsideAllowedRoots", "{tool}: {data}");
    }
    runtime.close().await;
}

/// Credential-named files stay withheld by the security path policy, and
/// every local tool says so truthfully: a policy denial with what it matched
/// and that no flag or config setting lifts it, never "check spelling" or
/// "retry the path". Source folders named `secrets/` or `private/` are not
/// withheld: only the filename pattern is.
#[tokio::test]
async fn policy_withheld_files_are_disclosed_never_advised_as_retry_or_spelling() {
    let workspace = Workspace::new();
    let secret = workspace.write(
        "src/credentials.json",
        "{\"name\": \"ISecretStorageService\"}\n",
    );
    workspace.write(
        "src/secrets/common/secrets.ts",
        "export interface ISecretStorageService {}\n",
    );
    workspace.write(
        "src/private/store.ts",
        "export interface ISecretStorageService {}\n",
    );
    workspace.write("src/other.ts", "export const other = 1;\n");
    let runtime = workspace.runtime(&[]);
    let root = workspace.workspace.join("src");
    // Named at the walked scope, never by the withheld file's own name.
    let scope = root
        .strip_prefix(&workspace.workspace)
        .expect("workspace scope");
    let notice = json!({"cause":"withheld","path":scope,"reason":"credential file"});
    let disclosed = |outcome: &octocode_native::response::outcome::ToolOutcome| {
        row_data(outcome)["skippedPaths"]["entries"]
            .as_array()
            .is_some_and(|entries| entries.contains(&notice))
    };

    let searched = call(
        &runtime,
        "localSearch",
        json!({"path":root,"matchString":["ISecretStorageService"]}),
    )
    .await
    .expect("search");
    let text = searched.structured_content.to_string();
    assert_ne!(row_status(&searched), "empty", "{text}");
    assert!(
        text.contains("src/secrets/common/secrets.ts")
            || text.contains("secrets/common/secrets.ts"),
        "{text}"
    );
    assert!(text.contains("private/store.ts"), "{text}");
    assert!(!text.contains("credentials.json"), "{text}");
    assert!(disclosed(&searched), "{text}");

    let fetched = call(&runtime, "localFetch", json!({"path":secret}))
        .await
        .expect("fetch");
    let data = row_data(&fetched);
    let text = fetched.structured_content.to_string();
    assert_eq!(data["errorCode"], "pathPolicyDenied", "{text}");
    assert!(
        data["error"]
            .as_str()
            .is_some_and(|error| error
                .contains("withheld by the security path policy (a credential file name)")),
        "{text}"
    );
    assert!(!text.to_lowercase().contains("then retry"), "{text}");
    assert!(
        data.get("next").is_none() && data.get("related").is_none(),
        "policy denial must not offer a bypass: {text}"
    );

    let listed = call(
        &runtime,
        "structureSearch",
        json!({"operation":"files","path":root,"extensions":["json"]}),
    )
    .await
    .expect("files");
    let text = listed.structured_content.to_string();
    assert!(disclosed(&listed), "{text}");
    let tree = call(
        &runtime,
        "structureSearch",
        json!({"operation":"tree","path":root}),
    )
    .await
    .expect("tree");
    let text = tree.structured_content.to_string();
    assert!(disclosed(&tree), "{text}");
    assert!(
        text.contains("secrets") && text.contains("private"),
        "{text}"
    );
    runtime.close().await;
}
