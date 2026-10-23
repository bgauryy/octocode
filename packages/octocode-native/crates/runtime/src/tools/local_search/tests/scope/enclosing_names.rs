//! Enclosing names and key paths on hits.

use super::*;

/// Enclosing names fill only the room the rendered page leaves: rows render
/// with the root's workspace-relative prefix, which the page cut charges, so
/// the names must too, or a deep root overflows the response window.
#[test]
fn enclosing_names_leave_room_for_the_rendered_path_prefix() {
    let workspace = tempfile::tempdir().expect("fixture directory");
    let deep = format!(
        "{}/src",
        ["a-rather-long-directory-name-for-paths"; 5].join("/")
    );
    let root = workspace.path().join(&deep);
    fs::create_dir_all(&root).expect("deep root");
    for index in 0..40 {
        fs::write(
            root.join(format!("module_{index:02}.rs")),
            format!("fn handler_{index:02}_with_a_long_name() {{\n    needle();\n}}\n\nfn other_{index:02}() {{\n    needle();\n}}\n"),
        )
        .expect("fixture");
    }
    // The row reserve exceeds half this window, so the page budget is its
    // floor: half the window.
    let window = 4_000;
    let budget = window / 2;
    let prefix = deep.len() + 1;
    let start = serde_json::to_value(ls_query(
        serde_json::json!({"path": root.to_string_lossy().into_owned(), "matchString": "needle"}),
        None,
    ))
    .expect("query");
    let (walk, bodies) = walk_search(workspace.path(), start, Some(window));
    for (index, body) in bodies.iter().enumerate() {
        let files = serde_json::to_string(&body["files"]).expect("files").len();
        let rendered = files + prefix * body["files"].as_array().map_or(0, Vec::len);
        assert!(
            rendered <= budget,
            "page {}: {rendered} > {budget}",
            index + 1
        );
    }
    assert_eq!(walk.rows.len(), 80);
}

/// A hit in a JSON or TOML file names its key path, one
/// `matchKeys` map per file row; other files carry none.
#[test]
fn json_and_toml_hits_name_their_key_paths() {
    let package = "{\n  \"scripts\": {\n    \"typescript\": \"tsc --noEmit\"\n  },\n  \"devDependencies\": {\n    \"typescript\": \"6.0.2\"\n  }\n}\n";
    let cargo = "[package]\nname = \"demo\"\n\n[workspace]\nmembers = [\n    \"typescript-x\",\n]\n\n[dependencies]\ntypescript = \"1\"\n";
    let body = search_fixture(
        &[
            ("package.json", package),
            ("Cargo.toml", cargo),
            ("notes.md", "typescript\n"),
        ],
        ls_query(
            serde_json::json!({"matchString": "typescript", "sort": "path"}),
            None,
        ),
    );
    let file = |name: &str| {
        body["files"]
            .as_array()
            .and_then(|files| files.iter().find(|file| file["path"] == name))
            .unwrap_or_else(|| panic!("{name}: {body}"))
            .clone()
    };
    assert_eq!(
        file("package.json")["matchKeys"],
        serde_json::json!({"3": "scripts.typescript", "6": "devDependencies.typescript"}),
        "{body}"
    );
    assert_eq!(
        file("Cargo.toml")["matchKeys"],
        serde_json::json!({"6": "workspace.members[0]", "10": "dependencies.typescript"}),
        "{body}"
    );
    assert!(file("notes.md").get("matchKeys").is_none(), "{body}");
}

/// The over-1 MiB note counts only files whose language has
/// declarations (a lockfile never had any), and says `1 hit row`.
#[test]
fn oversized_enclosing_note_skips_files_without_declarations() {
    let filler = "// padding line for an oversized source file\n".repeat(25_000);
    let body = search_fixture(
        &[
            ("uv.lock", &format!("needle = 1\n{filler}")),
            ("big.rs", &format!("fn f() {{ needle(); }}\n{filler}")),
        ],
        ls_query(serde_json::json!({"matchString": "needle"}), None),
    );
    let warnings = body["warnings"].to_string();
    assert!(
        warnings.contains("are not named in files over 1 MiB; 1 hit row there has none."),
        "{body}"
    );
    let lock_only = search_fixture(
        &[("uv.lock", &format!("needle = 1\n{filler}"))],
        ls_query(serde_json::json!({"matchString": "needle"}), None),
    );
    crate::tools::test_support::checks::assert_absent(&lock_only["warnings"], "1 MiB");
}
