//! The security path policy withholds a file by its credential
//! filename pattern or by secret content, never by the name of a directory
//! above it. One table drives every local tool.

use crate::support;

use serde_json::{Value, json};
use support::{Workspace, call, row_data};

use octocode_native::runtime::ToolRuntime;

const AWS_KEY: &str = "AKIAIOSFODNN7EXAMPLE";

/// How a fixture must read through every local tool.
enum Expect {
    Visible,
    /// Withheld by the credential filename pattern: the denial names it.
    WithheldByFilename,
}

/// `(relative path, content, expectation)`.
const TABLE: &[(&str, &str, Expect)] = &[
    (
        "src/private/util.ts",
        "export function needle() {}\n",
        Expect::Visible,
    ),
    (
        "src/secrets/store.ts",
        "export function needle() {}\n",
        Expect::Visible,
    ),
    ("secrets/README.md", "# needle\n", Expect::Visible),
    ("secrets/prod.env", "NEEDLE=1\n", Expect::WithheldByFilename),
    ("private/key.pem", "needle\n", Expect::WithheldByFilename),
    (
        "private/.env.local",
        "NEEDLE=1\n",
        Expect::WithheldByFilename,
    ),
    ("secrets/id_rsa", "needle\n", Expect::WithheldByFilename),
    (
        "secrets/credentials.json",
        "{\"needle\":1}\n",
        Expect::WithheldByFilename,
    ),
    (
        "private/release.keystore",
        "needle\n",
        Expect::WithheldByFilename,
    ),
];

fn fixture() -> Workspace {
    let workspace = Workspace::new();
    for (path, content, _) in TABLE {
        workspace.write(path, content);
    }
    workspace
}

/// A runtime whose language servers for the table's extensions are a missing
/// command: the path policy decides before discovery, and no installed
/// server (tsserver's type acquisition runs `npm install`) is ever started.
fn runtime(workspace: &Workspace) -> ToolRuntime {
    let servers: serde_json::Map<String, Value> = [
        ".ts",
        ".md",
        ".env",
        ".pem",
        ".local",
        ".json",
        ".keystore",
    ]
    .into_iter()
    .map(|extension| {
        (
            extension.to_owned(),
            json!({"command": "octocode-test-missing-lsp", "args": [], "languageId": "plaintext"}),
        )
    })
    .collect();
    // Outside the workspace, so the walks never list it.
    let config = workspace.home.join("lsp.json");
    std::fs::write(&config, json!({"languageServers": servers}).to_string()).expect("lsp config");
    workspace.runtime(&[("OCTOCODE_LSP_CONFIG", config.to_string_lossy().into_owned())])
}

/// The one-file query each tool takes for `path`.
fn file_query(tool: &str, path: &std::path::Path) -> Value {
    match tool {
        "localFetch" => json!({"path": path}),
        "localSearch" => json!({"path": path, "matchString": "needle", "includeHidden": true}),
        "structureSearch" => json!({"operation": "files", "path": path}),
        "astSearch" => {
            json!({"operation": "symbols", "path": path, "symbolName": "needle"})
        }
        "lspSearch" => json!({"operation": "documentSymbols", "path": path}),
        other => panic!("{other}"),
    }
}

const TOOLS: [&str; 5] = [
    "localFetch",
    "localSearch",
    "structureSearch",
    "astSearch",
    "lspSearch",
];

/// The denial message when `tool` refuses `path` by the path policy.
async fn denial(runtime: &ToolRuntime, tool: &str, path: &std::path::Path) -> Option<String> {
    let outcome = call(runtime, tool, file_query(tool, path))
        .await
        .unwrap_or_else(|error| panic!("{tool} {path:?}: {error:?}"));
    let data = row_data(&outcome);
    (data["errorCode"] == "pathPolicyDenied").then(|| {
        data["error"]
            .as_str()
            .unwrap_or_else(|| panic!("{tool}: denial without a message: {data}"))
            .to_owned()
    })
}

/// Every tool reads a source folder named `private/` or `secrets/`, and
/// withholds a credential-named file inside one, naming that reason.
#[tokio::test]
async fn every_local_tool_decides_by_filename_not_directory_name() {
    let workspace = fixture();
    let runtime = runtime(&workspace);
    for (relative, _, expect) in TABLE {
        let path = workspace.workspace.join(relative);
        for tool in TOOLS {
            let denied = denial(&runtime, tool, &path).await;
            match expect {
                Expect::Visible => {
                    assert_eq!(denied, None, "{tool} must read {relative}");
                }
                Expect::WithheldByFilename => {
                    let message =
                        denied.unwrap_or_else(|| panic!("{tool} must withhold {relative}"));
                    assert!(
                        message.contains("a credential file name"),
                        "{tool} {relative}: {message}"
                    );
                    assert!(
                        !message.contains("directory"),
                        "{tool} {relative}: {message}"
                    );
                }
            }
        }
    }
    runtime.close().await;
}

/// A walk over the whole workspace lists and searches the source folders,
/// skips the credential-named files, and discloses them as credential files
/// rather than as policy directories.
#[tokio::test]
async fn walks_keep_source_folders_and_disclose_only_credential_files() {
    let workspace = fixture();
    let runtime = runtime(&workspace);
    let root = &workspace.workspace;
    let withheld = TABLE
        .iter()
        .filter(|(_, _, expect)| matches!(expect, Expect::WithheldByFilename))
        .count();
    // Counted at the walked scope as credential files, never as policy
    // directories, and never by the withheld names.
    let notice = json!({"cause":"withheld","count":withheld,"path":".","reason":"credential file"});
    let disclosed = |outcome: &octocode_native::response::outcome::ToolOutcome| {
        row_data(outcome)["skippedPaths"]["entries"] == json!([notice])
    };

    let searched = call(
        &runtime,
        "localSearch",
        json!({"path": root, "matchString": "needle", "includeHidden": true}),
    )
    .await
    .expect("localSearch");
    let text = searched.structured_content.to_string();
    for (relative, _, expect) in TABLE {
        let seen = text.contains(relative);
        assert_eq!(
            seen,
            matches!(expect, Expect::Visible),
            "{relative}: {text}"
        );
    }
    assert!(disclosed(&searched), "{text}");

    let listed = call(
        &runtime,
        "structureSearch",
        json!({"operation": "files", "path": root, "includeHidden": true}),
    )
    .await
    .expect("structureSearch");
    let text = listed.structured_content.to_string();
    for (relative, _, expect) in TABLE {
        let name = relative.rsplit('/').next().unwrap_or(relative);
        assert_eq!(
            text.contains(name),
            matches!(expect, Expect::Visible),
            "{relative}: {text}"
        );
    }
    assert!(disclosed(&listed), "{text}");

    let matched = call(
        &runtime,
        "astSearch",
        json!({"operation": "match", "path": root, "language": "typescript", "pattern": "function needle() {}"}),
    )
    .await
    .expect("astSearch");
    let text = matched.structured_content.to_string();
    assert!(text.contains("src/private/util.ts"), "{text}");
    assert!(text.contains("src/secrets/store.ts"), "{text}");
    assert!(!text.contains("security-policy dir"), "{text}");
    runtime.close().await;
}

/// Content scanning still guards a visible file in a `secrets/` folder: the
/// AWS key is redacted from every read, whatever the directory is called.
#[tokio::test]
async fn secret_content_is_redacted_in_any_directory() {
    let workspace = Workspace::new();
    let body = format!("export const awsAccessKeyId = \"{AWS_KEY}\";\nexport const marker = 1;\n");
    let secret = workspace.write("src/secrets/aws.ts", &body);
    let plain = workspace.write("src/private/aws.ts", &body);
    let runtime = runtime(&workspace);
    for path in [&secret, &plain] {
        let fetched = call(&runtime, "localFetch", json!({"path": path}))
            .await
            .expect("localFetch");
        let text = fetched.structured_content.to_string();
        assert!(!text.contains(AWS_KEY), "{path:?}: {text}");
        assert!(text.contains("marker"), "{path:?}: {text}");
    }
    let searched = call(
        &runtime,
        "localSearch",
        json!({"path": workspace.workspace, "matchString": "awsAccessKeyId"}),
    )
    .await
    .expect("localSearch");
    let text = searched.structured_content.to_string();
    assert!(!text.contains(AWS_KEY), "{text}");
    assert!(text.contains("src/secrets/aws.ts"), "{text}");
    runtime.close().await;
}
