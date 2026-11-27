use super::*;

const NAMES: &[&str] = &["compile_flags.txt", "build/compile_commands.json"];

fn watch_of(file: &Path) -> ProjectConfigWatch {
    let mut watch = ProjectConfigWatch::new(NAMES);
    watch.observe(file);
    watch
}

#[test]
fn creating_changing_and_deleting_a_config_file_is_noticed_in_any_ancestor() {
    let root = tempfile::tempdir().expect("tempdir");
    let leaf = root.path().join("a/b");
    std::fs::create_dir_all(&leaf).expect("dirs");
    let source = leaf.join("x.cu");
    std::fs::write(&source, "").expect("source");
    let watch = watch_of(&source);
    assert!(!watch.changed(), "nothing changed yet");

    let flags = root.path().join("compile_flags.txt");
    std::fs::write(&flags, "-xcuda\n").expect("create");
    assert!(watch.changed(), "a config file created in an ancestor");

    let mut after_create = ProjectConfigWatch::new(NAMES);
    after_create.observe(&source);
    assert!(!after_create.changed());
    std::fs::write(&flags, "-xcuda\n-std=c++17\n").expect("edit");
    assert!(
        after_create.changed(),
        "a config file whose content changed"
    );

    let mut after_edit = ProjectConfigWatch::new(NAMES);
    after_edit.observe(&source);
    std::fs::remove_file(&flags).expect("delete");
    assert!(after_edit.changed(), "a config file deleted");
}

#[test]
fn a_nested_config_path_and_unrelated_files_are_told_apart() {
    let root = tempfile::tempdir().expect("tempdir");
    let source = root.path().join("x.cu");
    std::fs::write(&source, "").expect("source");
    let watch = watch_of(&source);
    std::fs::write(root.path().join("notes.txt"), "x").expect("unrelated");
    assert!(!watch.changed(), "a file the server does not read");
    std::fs::create_dir(root.path().join("build")).expect("build dir");
    std::fs::write(root.path().join("build/compile_commands.json"), "[]").expect("db");
    assert!(watch.changed(), "build/compile_commands.json");
}

/// Editors and build tools can replace a config while preserving its
/// modification time. Equal size and mtime do not prove it is unchanged.
#[cfg(unix)]
#[test]
fn a_same_size_config_replacement_restoring_mtime_invalidates_the_server() {
    use std::os::unix::fs::MetadataExt;

    let root = tempfile::tempdir().expect("tempdir");
    let source = root.path().join("x.cpp");
    std::fs::write(&source, "").expect("source");
    let flags = root.path().join("compile_flags.txt");
    std::fs::write(&flags, "-std=c++17\n").expect("initial flags");
    let before = std::fs::metadata(&flags).expect("initial metadata");
    let watch = watch_of(&source);
    assert!(!watch.changed());

    let replacement = root.path().join("replacement.txt");
    std::fs::write(&replacement, "-std=c++20\n").expect("replacement flags");
    std::fs::File::options()
        .write(true)
        .open(&replacement)
        .expect("replacement file")
        .set_modified(before.modified().expect("initial mtime"))
        .expect("preserve mtime");
    std::fs::rename(&replacement, &flags).expect("replace flags");

    let after = std::fs::metadata(&flags).expect("replacement metadata");
    assert_eq!(before.len(), after.len());
    assert_eq!(before.modified().unwrap(), after.modified().unwrap());
    assert_ne!(
        before.ino(),
        after.ino(),
        "a distinct file replaced the config"
    );
    assert!(watch.changed(), "the server must reload the changed flags");
}

#[test]
fn a_later_observation_keeps_the_older_baseline() {
    let root = tempfile::tempdir().expect("tempdir");
    let first = root.path().join("x.cu");
    std::fs::write(&first, "").expect("source");
    let mut watch = watch_of(&first);
    std::fs::write(root.path().join("compile_flags.txt"), "-xcuda\n").expect("create");
    watch.observe(&first);
    watch.observe(root.path());
    assert!(
        watch.changed(),
        "re-observing must not absorb a change the server has not seen"
    );
}

#[test]
fn a_new_directory_is_watched_from_the_moment_it_is_first_observed() {
    let root = tempfile::tempdir().expect("tempdir");
    let other = root.path().join("other");
    std::fs::create_dir(&other).expect("dir");
    let first = root.path().join("x.cu");
    std::fs::write(&first, "").expect("source");
    let mut watch = watch_of(&first);
    std::fs::write(other.join("compile_flags.txt"), "-xcuda\n").expect("existing");
    watch.observe(&other.join("y.cu"));
    assert!(
        !watch.changed(),
        "a file already there when its directory is first observed is the baseline"
    );
}

#[test]
fn a_server_without_config_files_watches_nothing() {
    let root = tempfile::tempdir().expect("tempdir");
    let source = root.path().join("x.ts");
    std::fs::write(&source, "").expect("source");
    let mut watch = ProjectConfigWatch::new(&[]);
    watch.observe(&source);
    assert!(!watch.is_active());
    assert!(!watch.changed());
}

/// Issue #482: clangd started with `--compile-commands-dir=build/debug`
/// reads that database for every file; rebuilding it reloads clangd.
#[test]
fn a_configured_database_directory_is_watched() {
    let root = tempfile::tempdir().expect("tempdir");
    let debug = root.path().join("build/debug");
    std::fs::create_dir_all(&debug).expect("debug");
    let source = root.path().join("src/a.cpp");
    std::fs::create_dir_all(source.parent().expect("src")).expect("src");
    std::fs::write(&source, "").expect("source");
    let config = crate::lsp::types::LspLanguageServerConfig {
        command: "clangd".into(),
        args: Some(vec!["--compile-commands-dir=build/debug".into()]),
        workspace_root: root.path().to_string_lossy().into_owned(),
        language_id: Some("cpp".into()),
        document_language_ids: Default::default(),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
        source: crate::lsp::types::ServerSource::Path,
    };
    let mut watch = ProjectConfigWatch::new(NAMES)
        .with_files(crate::lsp::clangd_database::fixed_database_files(&config));
    watch.observe(&source);
    assert!(!watch.changed(), "nothing changed yet");
    std::fs::write(debug.join("compile_commands.json"), "[]").expect("database");
    assert!(watch.changed(), "the configured database was created");
    watch.clear();
    watch.observe(&source);
    assert!(!watch.changed(), "a new baseline after clear");
}

/// Rebuilding an existing configured database (not only creating one)
/// reloads clangd.
#[test]
fn modifying_a_configured_database_is_a_change() {
    let root = tempfile::tempdir().expect("tempdir");
    let debug = root.path().join("build/debug");
    std::fs::create_dir_all(&debug).expect("debug");
    let database = debug.join("compile_commands.json");
    std::fs::write(&database, "[]").expect("database");
    let source = root.path().join("src/a.cpp");
    std::fs::create_dir_all(source.parent().expect("src")).expect("src");
    std::fs::write(&source, "").expect("source");
    let config = crate::lsp::types::LspLanguageServerConfig {
        command: "clangd".into(),
        args: Some(vec!["--compile-commands-dir=build/debug".into()]),
        workspace_root: root.path().to_string_lossy().into_owned(),
        language_id: Some("cpp".into()),
        document_language_ids: Default::default(),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
        source: crate::lsp::types::ServerSource::Path,
    };
    let mut watch = ProjectConfigWatch::new(NAMES)
        .with_files(crate::lsp::clangd_database::fixed_database_files(&config));
    watch.observe(&source);
    assert!(!watch.changed(), "an unchanged database is no change");
    std::fs::write(&database, "[{\"file\": \"a.cpp\"}]").expect("rebuild");
    assert!(watch.changed(), "the rebuilt database is a change");
}
