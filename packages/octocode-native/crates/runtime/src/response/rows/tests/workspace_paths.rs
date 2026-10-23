use super::*;
use crate::policy::path::PathPolicy;

/// A workspace with `packages/app/{src,tests}` and an allowed root
/// outside it.
struct Workspace {
    _dirs: (tempfile::TempDir, tempfile::TempDir),
    root: std::path::PathBuf,
    outside: std::path::PathBuf,
    paths: PathPolicy,
}

fn workspace() -> Workspace {
    let dir = tempfile::tempdir().expect("fixture");
    let other = tempfile::tempdir().expect("fixture");
    let root = std::fs::canonicalize(dir.path()).expect("root");
    let outside = std::fs::canonicalize(other.path()).expect("outside");
    for file in ["src/a.rs", "src/runtime/b.rs", "tests/x.ts"] {
        let path = root.join("packages/app").join(file);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(&path, "x").expect("file");
    }
    std::fs::write(outside.join("o.rs"), "x").expect("file");
    let paths = PathPolicy::new(crate::policy::path::PathPolicyConfig {
        workspace_root: Some(root.join("packages/app")),
        additional_roots: vec![outside.clone()],
        include_home: false,
        home_dir: None,
    })
    .expect("policy");
    Workspace {
        _dirs: (dir, other),
        root: root.join("packages/app"),
        outside,
        paths,
    }
}

/// Every relative row path must be what localFetch resolves: the
/// workspace root joined with the path.
fn assert_resolvable(ws: &Workspace, value: &Value, path: &str) {
    assert_eq!(value["root"], ws.root.to_string_lossy().as_ref(), "{value}");
    assert!(ws.paths.validate(path).is_ok(), "{path} in {value}");
}

/// Continuation queries name local paths relative to the workspace root
/// (`base`), which resolves them to the same files; outside paths stay.
#[test]
fn continuation_query_paths_are_workspace_relative() {
    let ws = workspace();
    let root = ws.root.to_string_lossy().into_owned();
    let outside = ws.outside.join("o.rs").to_string_lossy().into_owned();
    let row = json!({"index":0,"data":{
        "files":[{"path":format!("{root}/src/a.rs")}],
        "next":{"nextPage":{"tool":"localSearch","query":{"queries":[{"path":format!("{root}/src"),"matchString":"x","page":2}]}}},
        "hints":{"read":{"tool":"localFetch","query":{"queries":[{"path":outside.clone()}]}},
                 "refs":{"tool":"lspSearch","query":{"queries":[{"path":format!("{root}/src/a.rs"),"workspaceRoot":root.clone()}]}}}
    }});
    let value = envelope_in(
        vec![row],
        ToolId::LocalSearch,
        &[Some(&json!({"path": format!("{root}/src")}))],
        &ws.paths,
    );
    let data = &value["results"][0]["data"];
    assert_eq!(
        data["next"]["nextPage"]["query"]["queries"][0]["path"], "src",
        "{value}"
    );
    assert_eq!(
        data["hints"]["read"]["query"]["queries"][0]["path"],
        outside.as_str(),
        "{value}"
    );
    let refs = &data["hints"]["refs"]["query"]["queries"][0];
    assert_eq!(refs["path"], "src/a.rs", "{value}");
    assert_eq!(refs["workspaceRoot"], root.as_str(), "{value}");
    assert_resolvable(&ws, &value, "src");
}

/// With `extras: ["receipts"]`, local reads report the checked-out
/// commit once per response: shared when every row reads the same
/// repository, per row otherwise, and not at all outside a repository,
/// for other tools, or for rows that did not ask.
#[test]
fn local_rows_report_the_checked_out_head_once() {
    let ws = workspace();
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let git = ws.root.join(".git");
    std::fs::create_dir_all(git.join("refs/heads")).expect("git");
    std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").expect("head");
    std::fs::write(git.join("refs/heads/main"), format!("{sha}\n")).expect("ref");
    let root = ws.root.to_string_lossy().into_owned();
    let row =
        |index: u64| json!({"index":index,"data":{"files":[{"path":format!("{root}/src/a.rs")}]}});
    let src = json!({"path": format!("{root}/src"), "extras": ["receipts"]});
    let tests = json!({"path": format!("{root}/tests"), "extras": ["receipts"]});
    let unasked = json!({"path": format!("{root}/src")});
    let default = envelope_in(
        vec![row(0)],
        ToolId::LocalSearch,
        &[Some(&unasked)],
        &ws.paths,
    );
    assert!(default.get("shared").is_none(), "{default}");
    assert!(
        default["results"][0]["data"].get("commitSha").is_none(),
        "{default}"
    );
    for tool in [
        ToolId::LocalSearch,
        ToolId::LocalFetch,
        ToolId::StructureSearch,
    ] {
        let value = envelope_in(
            vec![row(0), row(1)],
            tool,
            &[Some(&src), Some(&tests)],
            &ws.paths,
        );
        assert_eq!(value["shared"]["commitSha"], sha, "{tool:?} {value}");
        assert!(
            value["results"][0]["data"].get("commitSha").is_none(),
            "{value}"
        );
    }
    let outside = json!({"path": ws.outside.to_string_lossy(), "extras": ["receipts"]});
    let mixed = envelope_in(
        vec![row(0), row(1)],
        ToolId::LocalSearch,
        &[Some(&src), Some(&outside)],
        &ws.paths,
    );
    assert!(mixed["shared"].get("commitSha").is_none(), "{mixed}");
    assert_eq!(mixed["results"][0]["data"]["commitSha"], sha, "{mixed}");
    assert!(
        mixed["results"][1]["data"].get("commitSha").is_none(),
        "{mixed}"
    );
    let other = envelope_in(vec![row(0)], ToolId::AstSearch, &[Some(&src)], &ws.paths);
    assert!(other.get("shared").is_none(), "{other}");
    let plain = envelope_in(
        vec![row(0)],
        ToolId::LocalSearch,
        &[Some(&outside)],
        &ws.paths,
    );
    assert!(plain.get("shared").is_none(), "{plain}");
}

#[test]
fn structure_and_ast_rows_are_workspace_relative_for_any_query_root() {
    let ws = workspace();
    let root = ws.root.to_string_lossy().into_owned();
    // `.` is the workspace root itself: rows lead with its own name.
    let name = ws
        .root
        .file_name()
        .expect("name")
        .to_string_lossy()
        .into_owned();
    for (tool, query, row, expected) in [
        (
            "structureSearch",
            json!({"path":"."}),
            json!({"path": name, "files":[{"path": format!("{name}/tests/x.ts")}]}),
            (".", "tests/x.ts"),
        ),
        (
            "structureSearch",
            json!({"path": format!("{root}/src")}),
            json!({"path":"src","files":[{"path":"src/runtime/b.rs"}]}),
            ("src", "src/runtime/b.rs"),
        ),
        (
            "astSearch",
            json!({"path":"src/runtime"}),
            json!({"path":"runtime","files":[{"path":"runtime/b.rs"}]}),
            ("src/runtime", "src/runtime/b.rs"),
        ),
        // A file root names the file relative to its parent.
        (
            "astSearch",
            json!({"path":"src/a.rs"}),
            json!({"path":"a.rs","files":[{"path":"a.rs"}]}),
            ("src/a.rs", "src/a.rs"),
        ),
    ] {
        let value = envelope_in(
            vec![json!({"index":0,"data":row})],
            ToolId::from_name(tool).expect("known tool"),
            &[Some(&query)],
            &ws.paths,
        );
        let data = &value["results"][0]["data"];
        assert_eq!(
            (
                data["path"].as_str().unwrap(),
                data["files"][0]["path"].as_str().unwrap()
            ),
            expected,
            "{tool} {query}"
        );
        assert_resolvable(&ws, &value, expected.1);
    }
}

/// structureSearch `files` groups name `dir` relative to the row's
/// `path` (like `tree`), so the envelope leaves them as they are: only
/// `path` resolves against `base`, and a batch never hoists a `dir`.
#[test]
fn structure_listing_dirs_stay_relative_to_the_row_path() {
    let ws = workspace();
    let src = json!({"path":"src"});
    let rows = vec![
        json!({"index":0,"data":{"path":"src","files":["a.rs (1)",
            {"dir":"runtime","files":["b.rs (1)"]}]}}),
        json!({"index":1,"data":{"path":"src","files":[{"dir":"runtime","files":["b.rs (1)"]}]}}),
    ];
    let value = envelope_in(
        rows,
        ToolId::StructureSearch,
        &[Some(&src), Some(&src)],
        &ws.paths,
    );
    assert!(value.get("shared").is_none(), "{value}");
    for row in 0..2 {
        let data = &value["results"][row]["data"];
        let files = data["files"].as_array().expect("files");
        let group = files.iter().find(|item| item.is_object()).expect("group");
        assert_eq!(group["dir"], "runtime", "{value}");
        assert_eq!(data["path"], "src", "{value}");
    }
    for path in ["src/a.rs", "src/runtime/b.rs"] {
        assert_resolvable(&ws, &value, path);
    }
}

#[test]
fn mixed_root_batches_resolve_every_row_even_after_a_rejected_row() {
    let ws = workspace();
    let outside = ws.outside.to_string_lossy().into_owned();
    let src = json!({"path":"src"});
    let nested = json!({"path": ws.root.join("src/runtime").to_string_lossy()});
    let far = json!({"path": outside});
    let rows = vec![
        json!({"index":0,"data":{"files":[{"path":"src/a.rs"}]}}),
        json!({"index":1,"status":"error","data":{"error":"x","errorCode":"invalidInput"}}),
        json!({"index":2,"data":{"symbols":[{"path":"runtime/b.rs"}]}}),
        json!({"index":3,"data":{"files":[{"path": format!("{}/o.rs", ws.outside.file_name().unwrap().to_string_lossy())}]}}),
    ];
    let value = envelope_in(
        rows,
        ToolId::AstSearch,
        &[Some(&src), None, Some(&nested), Some(&far)],
        &ws.paths,
    );
    assert_resolvable(&ws, &value, "src/a.rs");
    assert_eq!(value["results"][0]["data"]["files"][0]["path"], "src/a.rs");
    assert_eq!(
        value["results"][2]["data"]["symbols"][0]["path"],
        "src/runtime/b.rs"
    );
    // Outside the workspace a path stays absolute.
    let far_path = value["results"][3]["data"]["files"][0]["path"]
        .as_str()
        .unwrap();
    assert_eq!(far_path, format!("{outside}/o.rs"));
    assert!(ws.paths.validate(far_path).is_ok());
}

#[test]
fn local_search_and_lsp_rows_are_workspace_relative_and_outside_rows_absolute() {
    let ws = workspace();
    let root = ws.root.to_string_lossy().into_owned();
    let row = json!({"index":0,"data":{"files":[{"path": format!("{root}/src/runtime/b.rs")}],
        "next":{"nextPage":{"tool":"localSearch","query":{"queries":[{"path":"src/runtime"}]}}}}});
    let value = envelope_in(
        vec![row],
        ToolId::LocalSearch,
        &[Some(&json!({"path":"src/runtime"}))],
        &ws.paths,
    );
    assert_eq!(
        value["results"][0]["data"]["files"][0]["path"],
        "src/runtime/b.rs"
    );
    assert_eq!(
        value["results"][0]["data"]["next"]["nextPage"]["query"]["queries"][0]["path"],
        "src/runtime"
    );
    assert_resolvable(&ws, &value, "src/runtime/b.rs");

    // An empty row carries no path, but its lead's relative `path`
    // resolves against the workspace: the response names it as `root`.
    let empty = json!({"index":0,"status":"empty","data":{"files":[],
        "next":{"includeSkipped":{"tool":"localSearch","query":{"queries":[{"path": format!("{root}/src/runtime"),"includeIgnored":true}]}}}}});
    let value = envelope_in(
        vec![empty],
        ToolId::LocalSearch,
        &[Some(&json!({"path":"src/runtime"}))],
        &ws.paths,
    );
    assert_eq!(
        value["results"][0]["data"]["next"]["includeSkipped"]["query"]["queries"][0]["path"],
        "src/runtime"
    );
    assert_eq!(value["root"], root.as_str(), "{value}");

    let lsp = json!({"index":0,"data":{"type":"documentSymbols","uri":format!("file://{root}/src/a.rs")}});
    let value = envelope_in(vec![lsp], ToolId::LspSearch, &[Some(&json!({}))], &ws.paths);
    assert_eq!(value["results"][0]["data"]["path"], "src/a.rs");
    assert_resolvable(&ws, &value, "src/a.rs");

    let outside = format!("{}/o.rs", ws.outside.to_string_lossy());
    let row = json!({"index":0,"data":{"files":[{"path": outside}]}});
    let value = envelope_in(
        vec![row],
        ToolId::LocalSearch,
        &[Some(&json!({"path": ws.outside.to_string_lossy()}))],
        &ws.paths,
    );
    assert!(value.get("root").is_none(), "{value}");
    assert_eq!(
        value["results"][0]["data"]["files"][0]["path"],
        outside.as_str()
    );

    // LSP locations outside the workspace name the same `path` field,
    // absolute, beside workspace-relative ones.
    let lsp = json!({"index":0,"data":{"type":"callers","items":[
        {"name":"inside","uri":format!("file://{root}/src/a.rs")},
        {"name":"outside","uri":format!("file://{outside}")}
    ]}});
    let value = envelope_in(vec![lsp], ToolId::LspSearch, &[Some(&json!({}))], &ws.paths);
    let items = &value["results"][0]["data"]["items"];
    assert_eq!(items[0]["path"], "src/a.rs", "{value}");
    assert_eq!(items[1]["path"], outside.as_str(), "{value}");
    assert!(items[1].get("uri").is_none(), "{value}");
}
