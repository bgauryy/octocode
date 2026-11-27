use super::*;
use serde_json::json;

#[test]
fn paths_align_without_rewriting_source_provider_identity_or_executable_requests() {
    let dir = tempfile::tempdir().expect("workspace");
    let root = dir.path().canonicalize().expect("canonical workspace");
    let paths = crate::test_support::workspace_policy(&root);
    let file = root.join("src/lib.ts").to_string_lossy().into_owned();
    let command = root.join("server").to_string_lossy().into_owned();
    let uri = octocode_engine::lsp::uri::path_to_uri(&file).expect("URI");
    let source = format!("1\tconst path = {file:?};\n");
    let related = json!({"kind":"related","tool":"localFetch","input":{"queries":[{"path":file}]}});
    let retry = json!({"kind":"continue","tool":"lspSearch","input":{"queries":[{"path":file,"operation":"documentSymbols"}]}});
    let mut row = json!({
        "path":file, "workspaceRoot":root,
        "lsp":{"receipt":{"workspaceRoot":root,"command":command,"argv":[file]}},
        "matches":[{"path":file,"content":source,"uri":uri},{"path":"/external/library.ts"}],
        "files":[{"path":file,"matches":[{"sites":[{"path":file}]}]},{"dir":root.join("src")}],
        "symbols":[{"path":file,"children":[{"kind":"related","path":file}]}],
        "diagnostics":[{"relatedInformation":[{"location":{"path":file}}]}],
        "skippedPaths":{"entries":[{"path":file}]},
        "related":[related],"next":{"retry":retry}
    });
    shown_paths(false, &mut row, &paths, &Value::Null);
    for pointer in [
        "/path",
        "/matches/0/path",
        "/files/0/path",
        "/files/0/matches/0/sites/0/path",
        "/symbols/0/path",
        "/symbols/0/children/0/path",
        "/diagnostics/0/relatedInformation/0/location/path",
        "/skippedPaths/entries/0/path",
    ] {
        assert_eq!(
            row.pointer(pointer),
            Some(&json!("src/lib.ts")),
            "{pointer}: {row}"
        );
    }
    assert_eq!(row["files"][1]["dir"], "src", "{row}");
    assert_eq!(row["workspaceRoot"], ".");
    assert_eq!(row["lsp"]["receipt"]["workspaceRoot"], ".");
    assert!(
        row.get("root").is_none(),
        "the workspace is no stated root: {row}"
    );
    assert_eq!(row["matches"][1]["path"], "/external/library.ts");
    assert_eq!(row["matches"][0]["content"], source);
    assert_eq!(row["matches"][0]["uri"], uri);
    assert_eq!(row["lsp"]["receipt"]["command"], command);
    assert_eq!(row["lsp"]["receipt"]["argv"][0], file);
    // A related call shows its input: its paths read like every other
    // path. A continuation runs sealed as written.
    assert_eq!(
        row["related"][0]["input"]["queries"][0]["path"], "src/lib.ts",
        "{row}"
    );
    assert_eq!(row["next"]["retry"], retry);
    let normalized = row.clone();
    shown_paths(false, &mut row, &paths, &Value::Null);
    assert_eq!(row, normalized, "already-relative pages are stable");
}

#[test]
fn rows_outside_the_workspace_name_paths_from_their_root_stated_once() {
    let workspace = tempfile::tempdir().expect("workspace");
    let outside = tempfile::tempdir().expect("outside root");
    let workspace = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let tree = outside.path().canonicalize().expect("canonical root");
    let paths = PathPolicy::new(crate::policy::path::PathPolicyConfig {
        workspace_root: Some(workspace),
        additional_roots: vec![tree.clone()],
        ..Default::default()
    })
    .expect("path policy");
    let file = tree.join("src/a.ts").to_string_lossy().into_owned();
    let related = json!({"kind":"related","tool":"localFetch","input":{"queries":[{"path":file}]}});
    let mut row = json!({
        "files":[{"path":file},{"path":tree.join("b.ts")}],
        "skippedPaths":{"entries":[{"path":tree.join("vendor")}]},
        "related":[related]
    });
    shown_paths(false, &mut row, &paths, &Value::Null);
    assert_eq!(row["root"], json!(tree), "{row}");
    assert_eq!(row["files"][0]["path"], "src/a.ts", "{row}");
    assert_eq!(row["files"][1]["path"], "b.ts", "{row}");
    assert_eq!(row["skippedPaths"]["entries"][0]["path"], "vendor", "{row}");
    assert_eq!(
        row["related"][0], related,
        "calls keep absolute paths: {row}"
    );
    // The row's own path that repeats the request stays as asked, for the
    // response to drop as an echo; alone it states no root.
    let asked = json!({"path": file});
    let mut read = json!({"path": file, "content": "1\tx"});
    shown_paths(false, &mut read, &paths, &asked);
    assert_eq!(read, json!({"path": file, "content": "1\tx"}), "{read}");
    // A cursor has no visible query whose path the response could drop as an
    // echo. Its read must name the file relative to the same allowed root.
    let mut first = json!({"path": file, "content": "first", "next": {"nextPage": {
        "tool":"localFetch", "input":{"queries":[{"path": file}]}
    }}});
    shown_paths(false, &mut first, &paths, &asked);
    assert_eq!(first["path"], "src/a.ts");
    assert_eq!(first["root"], json!(tree));
    let resume = json!({"path": file, "readProgress": {"unit":"bytes","offset":32}});
    let mut page = json!({"path": file, "content": "continued"});
    shown_paths(false, &mut page, &paths, &resume);
    assert_eq!(page["path"], "src/a.ts");
    assert_eq!(page["root"], json!(tree));
    // A row path joined to the stated root names the file.
    let joined = Path::new(row["root"].as_str().expect("root"))
        .join(row["files"][0]["path"].as_str().expect("path"));
    assert_eq!(joined, Path::new(&file), "{row}");
    // A graph names its rows from its own snapshot root.
    let mut graph = json!({"root": tree, "skippedPaths":{"entries":[{"path":file}]}});
    shown_paths(true, &mut graph, &paths, &Value::Null);
    assert_eq!(graph["root"], json!(tree), "{graph}");
    assert_eq!(
        graph["skippedPaths"]["entries"][0]["path"],
        json!(file),
        "{graph}"
    );
}

/// A row states the checkout its evidence lies in beside the paths it now
/// names relatively, and a refusal states the asked path's: neither sends
/// the caller into `.git`. A graph row, naming its own root, states none.
#[test]
fn a_row_and_a_refusal_state_the_checkout_of_their_paths() {
    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
    let dir = tempfile::tempdir().expect("workspace");
    let root = dir.path().canonicalize().expect("canonical workspace");
    let git = root.join("repo/.git");
    std::fs::create_dir_all(git.join("refs/heads")).expect("git dir");
    std::fs::create_dir_all(root.join("repo/src")).expect("src");
    std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").expect("HEAD");
    std::fs::write(git.join("refs/heads/main"), format!("{SHA}\n")).expect("ref");
    std::fs::write(
        git.join("config"),
        "[remote \"origin\"]\n\turl = https://user:secret@example.com/o/r.git\n",
    )
    .expect("config");
    let file = root.join("repo/src/lib.rs").to_string_lossy().into_owned();
    let paths = crate::test_support::workspace_policy(&root);
    let identity = json!({"commitSha":SHA,"ref":"main","remote":"example.com/o/r"});
    let stated = |row: &Value| {
        let fields = row.as_object().expect("row");
        Value::Object(
            ["commitSha", "ref", "remote"]
                .iter()
                .filter_map(|field| Some(((*field).to_owned(), fields.get(*field)?.clone())))
                .collect(),
        )
    };

    let mut row = json!({"files":[{"path":file}]});
    shown_paths(false, &mut row, &paths, &Value::Null);
    assert_eq!(row["files"][0]["path"], "repo/src/lib.rs", "{row}");
    assert_eq!(stated(&row), identity, "{row}");

    let asked = json!({"path": git.join("config").to_string_lossy()});
    let mut refused = json!({"errorCode":"pathPolicyDenied","error":"withheld"});
    shown_paths(false, &mut refused, &paths, &asked);
    assert_eq!(stated(&refused), identity, "{refused}");
    assert!(!refused.to_string().contains("secret"), "{refused}");

    let mut graph = json!({"files":[{"path":file}]});
    shown_paths(true, &mut graph, &paths, &Value::Null);
    assert_eq!(stated(&graph), json!({}), "{graph}");
}
