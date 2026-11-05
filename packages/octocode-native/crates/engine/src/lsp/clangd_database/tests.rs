use super::*;
use crate::test_support::TmpDir;

fn config(root: &Path, args: &[&str], init: Option<serde_json::Value>) -> LspLanguageServerConfig {
    LspLanguageServerConfig {
        command: "clangd".into(),
        args: Some(args.iter().map(|arg| (*arg).to_owned()).collect()),
        workspace_root: root.to_string_lossy().into_owned(),
        language_id: Some("cpp".into()),
        initialization_options: init,
        env: None,
        max_memory_mb: None,
        source: crate::lsp::types::ServerSource::Path,
    }
}

/// clangd's lookup order and flag spellings, as its sources define them.
#[test]
fn database_search_follows_clangd() {
    let root = TmpDir::new();
    let debug = root.join("build/debug");
    let other = root.join("other");
    std::fs::create_dir_all(&debug).expect("debug");
    std::fs::create_dir_all(&other).expect("other");
    let fixed = |dir: &Path| DatabaseSearch::Fixed(dir.to_path_buf());
    let absolute = debug.to_string_lossy().into_owned();
    let file = root.join("not-a-dir");
    std::fs::write(&file, "").expect("file");
    let cases: Vec<(&str, Vec<&str>, Option<serde_json::Value>, DatabaseSearch)> = vec![
        (
            "= form",
            vec!["--compile-commands-dir=build/debug"],
            None,
            fixed(&debug),
        ),
        (
            "separate value",
            vec!["--compile-commands-dir", "build/debug"],
            None,
            fixed(&debug),
        ),
        (
            "single dash",
            vec!["-compile-commands-dir=build/debug"],
            None,
            fixed(&debug),
        ),
        (
            "single dash separate value",
            vec!["-compile-commands-dir", "build/debug"],
            None,
            fixed(&debug),
        ),
        (
            "empty = value is unset",
            vec!["--compile-commands-dir="],
            None,
            DatabaseSearch::Ancestors,
        ),
        (
            "empty separate value is unset",
            vec!["--compile-commands-dir", ""],
            None,
            DatabaseSearch::Ancestors,
        ),
        (
            "a later empty value unsets an earlier one",
            vec![
                "--compile-commands-dir=build/debug",
                "--compile-commands-dir=",
            ],
            None,
            DatabaseSearch::Ancestors,
        ),
        (
            "flag without a value",
            vec!["--compile-commands-dir"],
            None,
            DatabaseSearch::Ancestors,
        ),
        (
            "a value naming an existing file is accepted",
            vec!["--compile-commands-dir=not-a-dir"],
            None,
            fixed(&file),
        ),
        (
            "absolute",
            vec!["--compile-commands-dir", &absolute],
            None,
            fixed(&debug),
        ),
        (
            "missing dir falls back",
            vec!["--compile-commands-dir=nope"],
            None,
            DatabaseSearch::Ancestors,
        ),
        (
            "no flag",
            vec!["--background-index"],
            None,
            DatabaseSearch::Ancestors,
        ),
        (
            "init option beats the flag",
            vec!["--compile-commands-dir=build/debug"],
            Some(serde_json::json!({"compilationDatabasePath": "other"})),
            fixed(&other),
        ),
    ];
    for (case, args, init, expected) in cases {
        assert_eq!(
            database_search(&config(&root, &args, init)),
            expected,
            "{case}"
        );
    }
}

#[test]
fn a_fixed_search_finds_its_databases_and_ancestor_clangd_files() {
    let root = TmpDir::new();
    let src = root.join("src");
    let fixed_dir = root.join("out");
    std::fs::create_dir_all(&src).expect("src");
    std::fs::create_dir_all(fixed_dir.join("build")).expect("out");
    // Ancestor databases are not read under a fixed search.
    std::fs::write(root.join("compile_commands.json"), "[]").expect("root db");
    std::fs::write(root.join(".clangd"), "").expect(".clangd");
    std::fs::write(fixed_dir.join("build/compile_commands.json"), "[]").expect("build db");
    std::fs::write(fixed_dir.join("compile_flags.txt"), "-std=c++20").expect("flags");
    let search = DatabaseSearch::Fixed(fixed_dir.clone());
    assert_eq!(
        clangd_config_sources(&src, &search),
        vec![
            fixed_dir.join("build/compile_commands.json"),
            fixed_dir.join("compile_flags.txt"),
            root.join(".clangd"),
        ]
    );
    assert_eq!(
        clangd_config_sources(&src, &DatabaseSearch::Ancestors),
        vec![root.join("compile_commands.json"), root.join(".clangd")]
    );
}

/// Under the default search a database also counts in an ancestor's
/// `build/`, but `.clangd` and `compile_flags.txt` under `build/` configure
/// nothing.
#[test]
fn an_ancestor_search_reads_build_databases_only() {
    let root = TmpDir::new();
    let src = root.join("src");
    std::fs::create_dir_all(root.join("build")).expect("build");
    std::fs::create_dir_all(&src).expect("src");
    if !clangd_config_sources(&src, &DatabaseSearch::Ancestors).is_empty() {
        return; // an ancestor of the temp dir has one
    }
    std::fs::write(root.join("build/.clangd"), b"").expect("build .clangd");
    std::fs::write(root.join("build/compile_flags.txt"), b"").expect("build flags");
    assert!(clangd_config_sources(&src, &DatabaseSearch::Ancestors).is_empty());
    std::fs::write(root.join("build/compile_commands.json"), b"[]").expect("db");
    std::fs::write(src.join(".clangd"), b"").expect(".clangd");
    assert_eq!(
        clangd_config_sources(&src, &DatabaseSearch::Ancestors),
        [
            root.join("build/compile_commands.json"),
            src.join(".clangd")
        ]
    );
}

#[test]
fn a_configured_database_that_holds_nothing_is_a_problem() {
    let root = TmpDir::new();
    std::fs::create_dir_all(root.join("build/debug")).expect("dir");
    let problem = |args: &[&str]| configured_database_problem(&config(&root, args, None));
    assert_eq!(problem(&[]), None);
    let empty = problem(&["--compile-commands-dir=build/debug"]);
    assert!(empty.is_some_and(|problem| problem.contains("holds no compile_commands.json")));
    let absent = problem(&["--compile-commands-dir=nope"]);
    assert!(absent.is_some_and(|problem| problem.contains("ignores")));
    std::fs::write(root.join("build/debug/compile_commands.json"), "[]").expect("db");
    assert_eq!(problem(&["--compile-commands-dir", "build/debug"]), None);
}

#[test]
fn a_project_database_is_mirrored_under_the_home_with_the_index_on() {
    let root = TmpDir::new();
    let home = TmpDir::new();
    std::fs::create_dir_all(root.join("build/debug")).expect("dir");
    let database = root.join("build/debug/compile_commands.json");
    std::fs::write(
        &database,
        r#"[{"directory":"rel","file":"a.c","arguments":["cc","a.c"]}]"#,
    )
    .expect("db");
    let mut mirrored = config(
        &root,
        &["--compile-commands-dir", "build/debug", "--log=info"],
        None,
    );
    mirror_database(&mut mirrored, &home);
    let args = mirrored.args.clone().expect("args");
    assert!(args.contains(&"--background-index".to_owned()), "{args:?}");
    assert!(args.contains(&"--log=info".to_owned()), "{args:?}");
    let flags = args
        .iter()
        .filter(|arg| arg.contains("compile-commands-dir"))
        .collect::<Vec<_>>();
    assert_eq!(flags.len(), 1, "{args:?}");
    assert!(flags[0].contains(&*home.to_string_lossy()), "{args:?}");
    // Readers still see the project's own database.
    assert!(indexes_project(&mirrored));
    assert_eq!(configured_database_problem(&mirrored), None);
    let found = fixed_database_files(&mirrored);
    let real = database.canonicalize().expect("real");
    assert!(found.contains(&real), "{found:?}");
    // The copy resolves relative directories, and nothing lands in the project.
    prepare_mirror(&mirrored).expect("mirror");
    let dir = compile_commands_dir(&args).expect("flag");
    let copy =
        std::fs::read_to_string(Path::new(&dir).join("compile_commands.json")).expect("copy");
    assert!(
        copy.contains(&*root.canonicalize().expect("real").to_string_lossy()),
        "{copy}"
    );
    assert_eq!(
        std::fs::read_dir(root.join("build/debug"))
            .expect("list")
            .count(),
        1
    );
    // A changed database reaches the copy at the next start.
    std::fs::write(&database, "[]").expect("db");
    prepare_mirror(&mirrored).expect("mirror");
    assert_eq!(
        std::fs::read_to_string(Path::new(&dir).join("compile_commands.json")).expect("copy"),
        "[]"
    );
}

#[test]
fn no_mirror_without_a_database_or_against_the_users_choice() {
    let root = TmpDir::new();
    let home = TmpDir::new();
    let mut none = config(&root, &[], None);
    mirror_database(&mut none, &home);
    assert!(!indexes_project(&none));
    assert!(none.env.is_none());
    std::fs::write(root.join("compile_commands.json"), "[]").expect("db");
    for chosen in ["--background-index", "--background-index=false"] {
        let mut own = config(&root, &[chosen], None);
        mirror_database(&mut own, &home);
        assert_eq!(own.args.as_deref().expect("args"), [chosen]);
        assert_eq!(indexes_project(&own), chosen == "--background-index");
    }
    let mut option = config(
        &root,
        &[],
        Some(serde_json::json!({"compilationDatabasePath": "x"})),
    );
    mirror_database(&mut option, &home);
    assert!(option.env.is_none());
}

/// A repository's database cannot make the index read outside the project.
#[test]
fn a_mirror_leaves_out_entries_that_read_outside_the_project() {
    let root = TmpDir::new();
    let home = TmpDir::new();
    let real = root.canonicalize().expect("real");
    let dir = real.to_string_lossy();
    let entries = serde_json::json!([
        {"directory": dir, "file": "a.c", "arguments": ["cc", "a.c"]},
        {"directory": dir, "file": ".env.c", "arguments": ["cc", ".env.c"]},
        {"directory": dir, "file": ".ssh/k.c", "arguments": ["cc", ".ssh/k.c"]},
        {"directory": dir, "file": "/elsewhere/x.c", "arguments": ["cc", "x.c"]},
        {"directory": dir, "file": "../up.c", "arguments": ["cc", "up.c"]},
        {"directory": dir, "file": "b.c", "arguments": ["cc", "-include", "/elsewhere/h.h", "b.c"]},
        {"directory": dir, "file": "c.c", "command": "cc -include/elsewhere/h.h c.c"},
        {"directory": dir, "file": "d.c", "command": "cc -include own.h d.c"},
        {"directory": dir, "file": "prod.env", "arguments": ["cc", "-x", "c", "prod.env"]},
        {"directory": dir, "file": "server.pem", "arguments": ["cc", "-x", "c", "server.pem"]},
        {"directory": dir, "file": "e.c", "arguments": ["cc", "@flags.rsp", "e.c"]},
        {"directory": dir, "file": "f.c", "arguments": ["cc", "-Wp,-include,/elsewhere/h.h", "f.c"]},
        {"directory": dir, "file": "g.c", "arguments": ["cc", "-Xclang", "-include", "-Xclang", "/elsewhere/h.h", "g.c"]},
        {"directory": dir, "file": "link.c", "arguments": ["cc", "link.c"]},
        {"directory": dir, "file": "q.c", "command": "cc -include \"/elsewhere/q h.h\" -c q.c"},
        {"directory": dir, "file": "w.c", "command": "clang --driver-mode=cl /FI/elsewhere/h.h w.c"},
        {"directory": dir, "file": "x.c", "arguments": ["clang-cl", "-FI/elsewhere/h.h", "x.c"]},
        {"directory": dir, "file": "y.c", "arguments": ["clang", "/clang:-include", "/clang:/elsewhere/h.h", "y.c"]},
    ]);
    std::fs::write(root.join("compile_commands.json"), entries.to_string()).expect("db");
    #[cfg(unix)]
    {
        std::fs::write(root.join(".env.c"), "x").expect("file");
        std::os::unix::fs::symlink(root.join(".env.c"), root.join("link.c")).expect("link");
    }
    let mut mirrored = config(&root, &[], None);
    mirror_database(&mut mirrored, &home);
    prepare_mirror(&mirrored).expect("mirror");
    let names = |file: &str| real.join(file).to_string_lossy().into_owned();
    let mut listed = listed_files(&mirrored).into_keys().collect::<Vec<_>>();
    listed.sort();
    assert_eq!(listed, [names("a.c"), names("d.c")]);
    assert!(!has_index_shards(&mirrored), "nothing is indexed yet");
    let shards = compile_commands_dir(mirrored.args.as_deref().expect("args")).expect("flag");
    let shards = Path::new(&shards).join(".cache/clangd/index");
    std::fs::create_dir_all(&shards).expect("dir");
    std::fs::write(shards.join("a.c.0123ABCD.idx"), "x").expect("shard");
    assert!(has_index_shards(&mirrored));
    assert_eq!(
        indexed_names(&mirrored),
        std::collections::HashSet::from(["a.c".to_owned()])
    );
}
