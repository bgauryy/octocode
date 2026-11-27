use super::*;

fn write(package: &Path, path: &str, text: &str) -> PathBuf {
    let file = package.join(path);
    fs::create_dir_all(file.parent().expect("parent")).expect("directory");
    fs::write(&file, text).expect("source");
    file
}

fn declared(file: &Path) -> Vec<PathBuf> {
    let mut owners = cargo_package_includes(file, &|| false)
        .expect("package")
        .declared_by;
    owners.sort();
    owners
}

#[test]
fn same_named_production_and_test_modules_have_distinct_owners() {
    let package = TmpDir::new();
    write(&package, "Cargo.toml", "[package]\nname = 'p'\n");
    write(&package, "src/lib.rs", "mod flow;\n");
    let production = write(&package, "src/flow.rs", "mod files;\nmod tests;\n");
    let tests = write(&package, "src/flow/tests.rs", "mod files;\n");
    let production_files = write(&package, "src/flow/files.rs", "pub fn page() {}\n");
    let test_files = write(
        &package,
        "src/flow/tests/files.rs",
        "#[test] fn page() {}\n",
    );

    assert_eq!(declared(&production_files), vec![production]);
    assert_eq!(declared(&test_files), vec![tests]);
}

fn assert_module_base(manifest: &str, source: &str, crate_root: bool) {
    let package = TmpDir::new();
    write(&package, "Cargo.toml", manifest);
    let owner = write(&package, source, "mod child;\n");
    let parent = owner.parent().expect("parent");
    let sibling = parent.join("child.rs");
    let nested = parent
        .join(owner.file_stem().expect("stem"))
        .join("child.rs");
    fs::create_dir_all(nested.parent().expect("parent")).expect("directory");
    fs::write(&sibling, "").expect("sibling");
    fs::write(&nested, "").expect("nested");
    let (actual, unrelated) =
        if crate_root || owner.file_name().is_some_and(|name| name == "mod.rs") {
            (sibling, nested)
        } else {
            (nested, sibling)
        };
    assert_eq!(declared(&actual), vec![owner], "source: {source}");
    assert_eq!(
        declared(&unrelated),
        Vec::<PathBuf>::new(),
        "source: {source}"
    );
}

#[test]
fn ordinary_modules_use_only_their_stem_directory() {
    for source in [
        "src/feature.rs",
        "src/feature/lib.rs",
        "src/feature/main.rs",
    ] {
        assert_module_base("[package]\nname = 'p'\n", source, false);
    }
}

#[test]
fn crate_roots_and_mod_rs_use_only_their_parent_directory() {
    for source in [
        "src/lib.rs",
        "src/main.rs",
        "src/feature/mod.rs",
        "src/bin/cli.rs",
        "src/bin/cli/main.rs",
        "tests/scenario.rs",
        "tests/scenario/main.rs",
        "benches/performance.rs",
        "examples/demo.rs",
        "build.rs",
    ] {
        assert_module_base("[package]\nname = 'p'\n", source, true);
    }
}

#[test]
fn explicit_cargo_target_paths_are_crate_roots() {
    for target in ["lib", "bin", "test", "bench", "example"] {
        let heading = if target == "lib" {
            "[lib]".to_owned()
        } else {
            format!("[[{target}]]")
        };
        let manifest = format!(
            "[package]\nname = 'p'\n{heading}\nname = 'target'\npath = 'custom/entry.rs'\n"
        );
        assert_module_base(&manifest, "custom/entry.rs", true);
    }
    assert_module_base(
        "[package]\nname = 'p'\nbuild = 'custom/generate.rs'\n",
        "custom/generate.rs",
        true,
    );
}

#[test]
fn disabled_automatic_targets_are_ordinary_modules() {
    for (setting, source) in [
        ("autolib", "src/lib.rs"),
        ("autobins", "src/main.rs"),
        ("autobins", "src/bin/cli.rs"),
        ("autotests", "tests/scenario.rs"),
        ("autobenches", "benches/performance.rs"),
        ("autoexamples", "examples/demo.rs"),
        ("build", "build.rs"),
    ] {
        let manifest = format!("[package]\nname = 'p'\n{setting} = false\n");
        assert_module_base(&manifest, source, false);
    }
}

#[test]
fn explicitly_named_targets_remain_roots_with_auto_discovery_disabled() {
    assert_module_base(
        "[package]\nname = 'p'\nautotests = false\n[[test]]\nname = 'scenario'\n",
        "tests/scenario.rs",
        true,
    );
}

#[test]
fn edition_2015_disables_only_the_explicitly_configured_target_kind() {
    let manifest = "[package]\nname = 'p'\n[[bin]]\nname = 'other'\npath = 'entry.rs'\n";
    for source in [
        "src/lib.rs",
        "tests/scenario.rs",
        "benches/perf.rs",
        "examples/demo.rs",
    ] {
        assert_module_base(manifest, source, true);
    }
    assert_module_base(manifest, "src/main.rs", false);
    assert_module_base(manifest, "src/bin/cli.rs", false);
    assert_module_base("[package]\nname = 'p'\n[lib]\n", "src/main.rs", true);
    assert_module_base(
        "[package]\nname = 'p'\nedition = '2024'\n[[bin]]\nname = 'other'\npath = 'entry.rs'\n",
        "src/bin/cli.rs",
        true,
    );
}

#[test]
fn custom_target_paths_replace_inferred_paths_with_the_same_name() {
    let manifest = "[package]\nname = 'p'\nedition = '2024'\n[lib]\npath = 'custom/library.rs'\n[[bin]]\nname = 'p'\npath = 'entry.rs'\n[[test]]\nname = 'scenario'\npath = 'custom/check.rs'\n";
    for source in ["src/lib.rs", "src/main.rs", "tests/scenario.rs"] {
        assert_module_base(manifest, source, false);
    }
    for source in [
        "custom/library.rs",
        "entry.rs",
        "custom/check.rs",
        "src/bin/cli.rs",
    ] {
        assert_module_base(manifest, source, true);
    }
}

#[test]
fn build_script_toml_quotes_comments_and_missing_paths_are_respected() {
    let package = TmpDir::new();
    let custom = write(&package, "custom/generate.rs", "");
    write(&package, "build.rs", "");
    for value in ["'custom/generate.rs'", "\"custom/generate.rs\" # comment"] {
        write(
            &package,
            "Cargo.toml",
            &format!("[package]\nname = 'p'\nbuild = {value}\n"),
        );
        assert_eq!(cargo_package_build_script(&package), Some(custom.clone()));
    }
    for value in ["false # disabled", "'missing.rs'"] {
        write(
            &package,
            "Cargo.toml",
            &format!("[package]\nname = 'p'\nbuild = {value}\n"),
        );
        assert_eq!(cargo_package_build_script(&package), None);
    }
}

#[test]
fn separate_crate_roots_can_share_an_ordinary_support_module() {
    let package = TmpDir::new();
    write(&package, "Cargo.toml", "[package]\nname = 'p'\n");
    let first = write(&package, "tests/first.rs", "mod common;\n");
    let second = write(&package, "tests/second.rs", "mod common;\n");
    let common = write(&package, "tests/common/mod.rs", "pub fn helper() {}\n");
    assert_eq!(declared(&common), vec![first, second]);
}

#[test]
fn path_attributed_modules_resolve_children_from_their_parent_directory() {
    let package = TmpDir::new();
    write(&package, "Cargo.toml", "[package]\nname = 'p'\n");
    write(
        &package,
        "src/lib.rs",
        "#[path = \"feature.rs\"]\nmod renamed;\n",
    );
    let owner = write(&package, "src/feature.rs", "mod child;\n");
    let actual = write(&package, "src/child.rs", "");
    let unrelated = write(&package, "src/feature/child.rs", "");
    assert_eq!(declared(&actual), vec![owner]);
    assert_eq!(declared(&unrelated), Vec::<PathBuf>::new());
}

#[test]
fn a_source_loaded_normally_and_by_path_keeps_both_child_contexts() {
    let package = TmpDir::new();
    write(&package, "Cargo.toml", "[package]\nname = 'p'\n");
    write(
        &package,
        "src/lib.rs",
        "mod feature;\n#[path = \"feature.rs\"]\nmod renamed;\n",
    );
    let owner = write(&package, "src/feature.rs", "mod child;\n");
    let parent_child = write(&package, "src/child.rs", "");
    let stem_child = write(&package, "src/feature/child.rs", "");
    assert_eq!(declared(&parent_child), vec![owner.clone()]);
    assert_eq!(declared(&stem_child), vec![owner]);
}

#[test]
fn module_context_cycles_terminate_and_scan_limits_remain_visible() {
    let package = TmpDir::new();
    write(&package, "Cargo.toml", "[package]\nname = 'p'\n");
    let lib = write(&package, "src/lib.rs", "mod feature;\n");
    let owner = write(&package, "src/feature.rs", "mod child;\n");
    let child = write(
        &package,
        "src/feature/child.rs",
        "#[path = \"../feature.rs\"]\nmod cycle;\n",
    );
    assert_eq!(declared(&child), vec![owner]);
    let checks = std::cell::Cell::new(0);
    let limited = cargo_package_includes(&lib, &|| {
        checks.set(checks.get() + 1);
        checks.get() > 1
    })
    .expect("package");
    assert!(limited.has_more);
    assert!(limited.declared_by.len() <= 1);
    let zero = cargo_package_includes(&lib, &|| true).expect("package");
    assert!(zero.has_more);
    assert_eq!(zero.declared_by, Vec::<PathBuf>::new());
}

#[test]
fn cargo_target_defaults_use_the_inherited_workspace_edition() {
    let workspace = TmpDir::new();
    write(
        &workspace,
        "Cargo.toml",
        "[workspace]\nmembers = ['member']\n[workspace.package]\nedition = '2024'\n",
    );
    let package = workspace.join("member");
    write(
        &package,
        "Cargo.toml",
        "[package]\nname = 'p'\nedition.workspace = true\n[[bin]]\nname = 'other'\npath = 'entry.rs'\n",
    );
    let owner = write(&package, "src/bin/cli.rs", "mod child;\n");
    let child = write(&package, "src/bin/child.rs", "");
    let unrelated = write(&package, "src/bin/cli/child.rs", "");
    assert_eq!(declared(&child), vec![owner]);
    assert_eq!(declared(&unrelated), Vec::<PathBuf>::new());
}
