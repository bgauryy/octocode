//! The security path policy withholds a file by its credential
//! filename pattern or by secret content, never by the name of a directory
//! above it. One table drives every local tool.

use crate::support;

use serde_json::{Value, json};
use support::{Workspace, call, row_data};

use octocode_native::runtime::ToolRuntime;

#[cfg(unix)]
mod symlinks;

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
        "src/credentials/hasher.ts",
        "export function needle() {}\n",
        Expect::Visible,
    ),
    (
        "src/source.pem/module.ts",
        "export function needle() {}\n",
        Expect::Visible,
    ),
    (
        "src/source.key/module.ts",
        "export function needle() {}\n",
        Expect::Visible,
    ),
    (
        "src/.env.production/module.ts",
        "export function needle() {}\n",
        Expect::Visible,
    ),
    (
        "src/id_rsa.old/module.ts",
        "export function needle() {}\n",
        Expect::Visible,
    ),
    (
        "src/credentials/.secrets/module.ts",
        "export function needle() {}\n",
        Expect::Visible,
    ),
    (
        "src/credentials/credentials",
        "needle\n",
        Expect::WithheldByFilename,
    ),
    (
        "src/credentials/deploy.pem",
        "needle\n",
        Expect::WithheldByFilename,
    ),
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
    let config = workspace.home.join("lsp-servers.json");
    std::fs::write(&config, json!({"languageServers": servers}).to_string()).expect("lsp config");
    workspace.runtime(&[])
}

/// The one-file query each tool takes for `path`.
fn file_query(tool: &str, path: &std::path::Path) -> Value {
    match tool {
        "localFetch" => json!({"path": path}),
        "localSearch" => json!({"path": path, "matchString": ["needle"], "includeHidden": true}),
        "structureSearch" => json!({"operation": "files", "path": path}),
        "astSearch" => {
            json!({"operation": "symbols", "path": path})
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
    // Disclosed as the directories that hold credential files (one entry per
    // directory, its count the files it holds), never as policy directories
    // and never by the withheld names; the counts sum to the total.
    let disclosed = |outcome: &octocode_native::response::outcome::ToolOutcome| {
        let data = row_data(outcome);
        let held = data["skippedPaths"]["entries"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|entry| entry["cause"] == "withheld" && entry["reason"] == "credential file")
            .collect::<Vec<_>>();
        let mut holders = held.iter().map(|entry| &entry["path"]).collect::<Vec<_>>();
        holders.sort_by_key(ToString::to_string);
        holders.dedup();
        holders.len() == held.len()
            && held
                .iter()
                .filter_map(|entry| entry["count"].as_u64())
                .sum::<u64>()
                == withheld as u64
    };

    let searched = call(
        &runtime,
        "localSearch",
        json!({"path": root, "matchString": ["needle"], "includeHidden": true}),
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
        let (dir, name) = relative.rsplit_once('/').expect("fixture parent");
        let seen = row_data(&listed)["files"]
            .as_array()
            .expect("file groups")
            .iter()
            .any(|group| {
                group["dir"] == dir
                    && group["files"]
                        .as_array()
                        .expect("group files")
                        .iter()
                        .any(|file| file == name)
            });
        assert_eq!(
            seen,
            matches!(expect, Expect::Visible),
            "{relative}: {text}"
        );
    }
    assert!(disclosed(&listed), "{text}");

    let matched = call(
        &runtime,
        "astSearch",
        json!({"operation": "match", "path": root, "includeHidden": true, "parser": "typescript", "rule": {"pattern": "function needle() {}"}}),
    )
    .await
    .expect("astSearch");
    let text = matched.structured_content.to_string();
    for (relative, _, expect) in TABLE {
        if matches!(expect, Expect::Visible) && relative.ends_with(".ts") {
            assert!(text.contains(relative), "{relative}: {text}");
        }
    }
    assert!(!text.contains("security-policy dir"), "{text}");
    runtime.close().await;
}

#[tokio::test]
async fn credential_directories_are_valid_walk_roots_for_every_discovery_tool() {
    let workspace = fixture();
    let runtime = runtime(&workspace);
    for name in ["credentials", "source.pem", ".env.production", "id_rsa.old"] {
        let path = workspace.workspace.join("src").join(name);
        for (tool, query) in [
            (
                "localSearch",
                json!({"path": path, "matchString": ["needle"], "includeHidden": true}),
            ),
            (
                "structureSearch",
                json!({"path": path, "operation": "files", "includeHidden": true}),
            ),
            (
                "astSearch",
                json!({"path": path, "operation": "symbols", "includeHidden": true}),
            ),
        ] {
            let outcome = call(&runtime, tool, query).await.expect(tool);
            let data = row_data(&outcome);
            assert!(data.get("error").is_none(), "{tool} {name}: {data}");
            assert!(data.to_string().contains(".ts"), "{tool} {name}: {data}");
        }
    }
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
    let credentials = workspace.write("src/credentials/aws.ts", &body);
    let runtime = runtime(&workspace);
    for path in [&secret, &plain, &credentials] {
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
        json!({"path": workspace.workspace, "matchString": ["awsAccessKeyId"]}),
    )
    .await
    .expect("localSearch");
    let text = searched.structured_content.to_string();
    assert!(!text.contains(AWS_KEY), "{text}");
    assert!(text.contains("src/secrets/aws.ts"), "{text}");
    assert!(text.contains("src/credentials/aws.ts"), "{text}");
    runtime.close().await;
}

/// A directory holding a searched file and a withheld one is disclosed as
/// the directory that holds the withheld file, never as the file (B143): the
/// withheld file's name appears nowhere in the result.
#[tokio::test]
async fn a_directory_beside_a_withheld_file_is_disclosed_as_its_holder() {
    let workspace = Workspace::new();
    workspace.write("conf/app.properties", "needle=1\n");
    workspace.write("conf/.env.sample", "needle=2\n");
    let runtime = runtime(&workspace);
    let searched = call(
        &runtime,
        "localSearch",
        json!({"path": workspace.workspace, "matchString": ["needle"], "includeHidden": true}),
    )
    .await
    .expect("localSearch");
    let text = searched.structured_content.to_string();
    assert!(text.contains("app.properties"), "{text}");
    assert!(!text.contains(".env.sample"), "{text}");
    let data = row_data(&searched);
    let held = data["skippedPaths"]["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| entry["cause"] == "withheld")
        .collect::<Vec<_>>();
    assert_eq!(held.len(), 1, "{text}");
    assert_eq!(held[0]["path"], "conf", "{text}");
    assert_eq!(held[0]["reason"], "credential file", "{text}");
    assert!(held[0].get("count").is_none(), "{text}");
    runtime.close().await;
}

/// A directory holding several withheld files is one entry whose `count` is
/// how many it holds, from the walk root, and no file name appears (B143).
#[tokio::test]
async fn a_directory_holding_several_withheld_files_is_one_counted_entry() {
    let withheld = [".env", ".env.sample", "id_rsa", "server.pem"];
    let workspace = Workspace::new();
    workspace.write("conf/app.properties", "needle=1\n");
    for name in withheld {
        workspace.write(&format!("conf/{name}"), "needle=2\n");
    }
    let runtime = runtime(&workspace);
    let searched = call(
        &runtime,
        "localSearch",
        json!({"path": workspace.workspace, "matchString": ["needle"], "includeHidden": true}),
    )
    .await
    .expect("localSearch");
    let text = searched.structured_content.to_string();
    for name in withheld {
        assert!(!text.contains(name), "{name} named: {text}");
    }
    let data = row_data(&searched);
    let held = data["skippedPaths"]["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| entry["cause"] == "withheld")
        .collect::<Vec<_>>();
    assert_eq!(held.len(), 1, "{text}");
    assert_eq!(held[0]["path"], "conf", "{text}");
    assert_eq!(held[0]["reason"], "credential file", "{text}");
    assert_eq!(held[0]["count"], withheld.len(), "{text}");
    runtime.close().await;
}
