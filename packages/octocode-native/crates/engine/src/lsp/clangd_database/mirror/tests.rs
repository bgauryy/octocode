use super::*;
use crate::test_support::TmpDir;
use serde_json::{Value, json};

const FAKE_TOKEN: &str = "ghp_Zz9Yy8Xx7Ww6Zz9Yy8Xx7Ww6Zz9Yy8Xx7Ww6";

fn mirrored_config(root: &Path, home: &Path) -> LspLanguageServerConfig {
    let mut config = LspLanguageServerConfig {
        command: "clangd".into(),
        args: Some(Vec::new()),
        workspace_root: root.to_string_lossy().into_owned(),
        language_id: Some("c".into()),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
        source: crate::lsp::types::ServerSource::Path,
    };
    mirror_database(&mut config, home);
    config
}

/// The mirror's folder and the entries written for `entries` in a project at `root`.
fn mirror_of(root: &Path, home: &Path, entries: &Value) -> (PathBuf, Vec<Value>) {
    std::fs::write(root.join("compile_commands.json"), entries.to_string()).expect("db");
    let config = mirrored_config(root, home);
    prepare_mirror(&config).expect("mirror");
    let dir = mirror_directory(&config).expect("mirror folder");
    let copy = std::fs::read(dir.join("compile_commands.json")).expect("copy");
    (dir, serde_json::from_slice(&copy).expect("json"))
}

fn kept_files(entries: &[Value]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| entry["file"].as_str().expect("file").to_owned())
        .collect()
}

/// The token is gone from every field, and the flag stays: the macro is still
/// defined and the file still compiles under the same switches.
#[test]
fn a_mirror_holds_no_secret_and_keeps_the_flag() {
    let root = TmpDir::new();
    let home = TmpDir::new();
    let dir = root.canonicalize().expect("real");
    let dir = dir.to_string_lossy();
    let (folder, entries) = mirror_of(
        &root,
        &home,
        &json!([
            {"directory": dir, "file": "a.c",
             "command": format!("clang -c -DAPI_TOKEN=\\\"{FAKE_TOKEN}\\\" -DPLAIN=1 a.c")},
            {"directory": dir, "file": "b.c",
             "arguments": ["clang", "-c", format!("-DAPI_TOKEN=\"{FAKE_TOKEN}\""), "b.c"],
             "command": format!("clang {FAKE_TOKEN}"),
             "output": format!("{FAKE_TOKEN}.o"),
             "note": FAKE_TOKEN},
            {"directory": dir, "file": "c.c", "command": "clang -c -DPLAIN=1 c.c"},
        ]),
    );
    let copy = std::fs::read_to_string(folder.join("compile_commands.json")).expect("copy");
    assert!(!copy.contains("ghp_"), "{copy}");
    let flags = |entry: &Value| {
        entry["arguments"]
            .as_array()
            .expect("arguments")
            .iter()
            .map(|word| word.as_str().expect("word").to_owned())
            .collect::<Vec<_>>()
    };
    for entry in &entries[..2] {
        let words = flags(entry);
        let define = words
            .iter()
            .find(|word| word.starts_with("-DAPI_TOKEN="))
            .unwrap_or_else(|| panic!("the flag stays: {words:?}"));
        assert!(define.contains("[REDACTED"), "{define}");
        assert!(define.starts_with("-DAPI_TOKEN=\""), "{define}");
    }
    assert!(flags(&entries[0]).contains(&"-DPLAIN=1".to_owned()));
    // A clean command is copied as written.
    assert_eq!(entries[2]["command"], "clang -c -DPLAIN=1 c.c");
}

/// A redacted path would name no file, so the entry leaves instead.
#[test]
fn an_entry_whose_path_spells_a_secret_is_left_out() {
    let root = TmpDir::new();
    let home = TmpDir::new();
    let dir = root.canonicalize().expect("real");
    let dir = dir.to_string_lossy();
    let (folder, entries) = mirror_of(
        &root,
        &home,
        &json!([
            {"directory": dir, "file": format!("{FAKE_TOKEN}.c"), "arguments": ["clang", "x.c"]},
            {"directory": format!("{dir}/{FAKE_TOKEN}"), "file": "y.c", "arguments": ["clang", "y.c"]},
            {"directory": dir, "file": "z.c", "arguments": ["clang", "z.c"]},
        ]),
    );
    assert_eq!(kept_files(&entries), ["z.c"]);
    let copy = std::fs::read_to_string(folder.join("compile_commands.json")).expect("copy");
    assert!(!copy.contains("ghp_"), "{copy}");
}

/// Flags that name no secret are not touched, whatever their words look like: a
/// changed value would change what the index sees.
#[test]
fn ordinary_values_are_not_taken_for_secrets() {
    let root = TmpDir::new();
    let home = TmpDir::new();
    let dir = root.canonicalize().expect("real");
    let dir = dir.to_string_lossy();
    let words = [
        "clang",
        "-DGIT_SHA=da39a3ee5e6b4b0d3255bfef95601890afd80709",
        "-DTOKEN_SIZE=32",
        "-DAPI_KEY_LENGTH=64",
        "-DPASSWORD_MIN=8",
        "-DMAX_TOKENS=4096",
        "-DAUTH_HEADER=\"Authorization\"",
        "-DVERSION=\"1.2.3\"",
        "-I/nix/store/5rnfzla9kcx4mj5zdc7nlnv8na1najvg-openssl-3.0.8-dev/include",
        "-isystem",
        "/opt/sdk/da39a3ee5e6b4b0d3255bfef95601890afd80709/include",
        "a.c",
    ];
    let (_, entries) = mirror_of(
        &root,
        &home,
        &json!([{"directory": dir, "file": "a.c", "arguments": words}]),
    );
    assert_eq!(entries[0]["arguments"], json!(words));
}

#[cfg(unix)]
fn mode_of(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::symlink_metadata(path)
        .expect("metadata")
        .permissions()
        .mode()
        & 0o777
}

/// The copy is owner-only from its first byte, and its folder keeps every other
/// user out of what clangd writes beside it at its own default modes.
#[cfg(unix)]
#[test]
fn a_mirror_is_owner_only_and_a_wider_earlier_one_is_tightened() {
    use std::os::unix::fs::PermissionsExt;
    let root = TmpDir::new();
    let home = TmpDir::new();
    let entries = json!([]);
    let (folder, _) = mirror_of(&root, &home, &entries);
    let copy = folder.join("compile_commands.json");
    assert_eq!(mode_of(&folder), 0o700);
    assert_eq!(mode_of(&copy), 0o600);
    assert_eq!(
        std::fs::read_dir(&folder).expect("list").count(),
        1,
        "no staged file stays behind"
    );
    // An earlier copy with the same content, at wider modes.
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).expect("dir");
    std::fs::set_permissions(&copy, std::fs::Permissions::from_mode(0o644)).expect("file");
    mirror_of(&root, &home, &entries);
    assert_eq!(mode_of(&folder), 0o700);
    assert_eq!(mode_of(&copy), 0o600);
}

/// A link planted at the copy's name is replaced, never written through or kept.
#[cfg(unix)]
#[test]
fn a_link_at_the_copy_is_replaced_not_followed() {
    let root = TmpDir::new();
    let home = TmpDir::new();
    let elsewhere = TmpDir::new();
    let outside = elsewhere.join("kept.json");
    let entries = json!([]);
    let (folder, _) = mirror_of(&root, &home, &entries);
    let copy = folder.join("compile_commands.json");
    std::fs::write(&outside, "[]").expect("outside");
    std::fs::remove_file(&copy).expect("remove");
    std::os::unix::fs::symlink(&outside, &copy).expect("link");
    mirror_of(&root, &home, &entries);
    let meta = std::fs::symlink_metadata(&copy).expect("metadata");
    assert!(meta.file_type().is_file(), "a real file again");
    std::fs::write(root.join("compile_commands.json"), "[{}]").expect("db");
    let config = mirrored_config(&root, &home);
    prepare_mirror(&config).expect("mirror");
    assert_eq!(std::fs::read_to_string(&outside).expect("outside"), "[]");
}

/// A repository's database is untrusted input: a malformed one starts nothing it
/// cannot describe, and never stops the server.
#[test]
fn entries_that_are_not_objects_are_left_out() {
    let root = TmpDir::new();
    let home = TmpDir::new();
    let dir = root.canonicalize().expect("real");
    let (_, entries) = mirror_of(
        &root,
        &home,
        &json!([
            "a string", 7, null, ["a.c"],
            {"directory": dir.to_string_lossy(), "file": "a.c", "arguments": ["clang", "a.c"]},
        ]),
    );
    assert_eq!(kept_files(&entries), ["a.c"]);
}

mod flags;
