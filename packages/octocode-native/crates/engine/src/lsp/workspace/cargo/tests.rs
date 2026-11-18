use super::*;

fn write(root: &Path, relative: &str, content: &str) -> PathBuf {
    let directory = root.join(relative);
    std::fs::create_dir_all(directory.join("src")).expect("package directory");
    std::fs::write(directory.join(CARGO_MANIFEST), content).expect("manifest");
    directory
}

fn resolved(package: &Path) -> PathBuf {
    PathBuf::from(
        super::super::resolve_workspace_root_for_file(
            package.join("src/lib.rs").to_string_lossy().into_owned(),
        )
        .expect("LSP root"),
    )
}

/// A members glob reads only the pattern: the root never enters it, so
/// no root spelling (a separator the glob dialect reads otherwise, bytes
/// that are not UTF-8, glob characters) changes what it matches.
#[cfg(unix)]
#[test]
fn a_members_glob_matches_below_any_root_spelling() {
    use std::os::unix::ffi::OsStrExt;
    for root in [
        PathBuf::from(std::ffi::OsStr::from_bytes(b"/w\xff/root")),
        PathBuf::from("/w[x]/{a,b} \\c"),
    ] {
        assert!(
            matches(&root, "crates/*", &root.join("crates/member")),
            "{root:?}"
        );
        assert!(
            matches(&root, "crates/**", &root.join("crates/a/b")),
            "{root:?}"
        );
        let outside = lexical(&root.join("../elsewhere/m"));
        assert!(matches(&root, "../elsewhere/m", &outside), "{root:?}");
        assert!(
            !matches(&root, "crates/*", &root.join("crates/a/b")),
            "{root:?}"
        );
        assert!(
            !matches(&root, "crates/*", &root.join("vendor/member")),
            "{root:?}"
        );
        // A glob names directories below its base, never the base.
        for glob in ["crates/*", "crates/**"] {
            assert!(
                !matches(&root, glob, &root.join("crates")),
                "{root:?} {glob}"
            );
        }
    }
}

#[test]
fn cargo_membership_respects_globs_exclusions_and_nested_workspaces() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let root = write(
        temporary.path(),
        "workspace",
        "[workspace]\nmembers = [\n 'crates/*', # members may be multiline\n]\nexclude = ['crates/excluded']\n",
    );
    let package = "[package]\nname = 'member'\nversion = '0.1.0'\n";
    let member = write(&root, "crates/member", package);
    let excluded = write(&root, "crates/excluded", package);
    let unrelated = write(&root, "vendor/unrelated", package);
    let nested = write(&root, "crates/nested", "[workspace]\nmembers = ['inner']\n");
    let inner = write(&nested, "inner", package);
    assert_eq!(resolved(&member), root);
    assert_eq!(resolved(&excluded), excluded);
    assert_eq!(resolved(&unrelated), unrelated);
    assert_eq!(resolved(&inner), nested);
}

#[test]
fn cargo_membership_follows_explicit_workspace_outside_the_package_ancestors() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let root = write(
        temporary.path(),
        "workspace",
        "[workspace]\nmembers = ['../elsewhere/member']\n",
    );
    let member = write(
        temporary.path(),
        "elsewhere/member",
        "[package]\nname = 'member'\nworkspace = '../../workspace'\n",
    );
    assert_eq!(resolved(&member), root);
}

#[test]
fn cargo_membership_follows_transitive_and_inherited_path_dependencies() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let root = write(
        temporary.path(),
        "workspace[checkout]",
        "[workspace]\nmembers = ['crates/*']\nexclude = ['support/excluded']\n[workspace.dependencies]\nshared = { path = 'support/shared' }\n",
    );
    write(
        &root,
        "crates/app",
        "[package]\nname = 'app'\n[dependencies]\nshared.workspace = true\n[target.'cfg(unix)'.build-dependencies]\nbridge = { path = '../../support/bridge' }\n",
    );
    let shared = write(&root, "support/shared", "[package]\nname = 'shared'\n");
    write(
        &root,
        "support/bridge",
        "[package]\nname = 'bridge'\n[dev-dependencies]\nleaf = { path = '../leaf' }\nexcluded = { path = '../excluded' }\n",
    );
    let leaf = write(&root, "support/leaf", "[package]\nname = 'leaf'\n");
    let excluded = write(&root, "support/excluded", "[package]\nname = 'excluded'\n");
    assert_eq!(resolved(&shared), root);
    assert_eq!(resolved(&leaf), root);
    assert_eq!(resolved(&excluded), excluded);
}

#[test]
fn cargo_exclusions_are_literal_prefixes_overridden_only_by_exact_members() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let root = write(
        temporary.path(),
        "workspace",
        "[workspace]\nmembers = ['app', 'vendor/explicit', 'vendor/*']\nexclude = ['vendor', 'support/*']\n",
    );
    write(
        &root,
        "app",
        "[package]\nname = 'app'\n[dependencies]\nleaf = { path = '../support/leaf' }\n",
    );
    let package = "[package]\nname = 'p'\n";
    let explicit = write(&root, "vendor/explicit", package);
    let excluded = write(&root, "vendor/expanded", package);
    let leaf = write(&root, "support/leaf", package);
    assert_eq!(resolved(&explicit), root);
    assert_eq!(resolved(&excluded), excluded);
    assert_eq!(
        resolved(&leaf),
        root,
        "exclude entries are not glob patterns"
    );
}

#[test]
fn cargo_workspace_headers_inside_strings_do_not_declare_a_workspace() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let root = write(
        temporary.path(),
        "package",
        "[package]\nname = 'p'\ndescription = '''\n[workspace]\n'''\n",
    );
    assert!(!declares_cargo_workspace(&root));
    assert_eq!(resolved(&root), root);
}

/// A manifest is parsed once while it is unchanged, and again once edited.
#[test]
fn a_manifest_is_parsed_once_until_it_changes() {
    let root = crate::test_support::TmpDir::new();
    let package = write(&root.0, "pkg", "[package]\nname = \"a\"\n");
    crate::stamp::settle_written_so_far();
    let first = cargo_manifest(&package).expect("parsed");
    let again = cargo_manifest(&package).expect("parsed");
    assert!(
        Arc::ptr_eq(&first, &again),
        "an unchanged manifest is not parsed again"
    );
    std::fs::write(package.join(CARGO_MANIFEST), "[package]\nname = \"bb\"\n").expect("edit");
    crate::stamp::settle_written_so_far();
    let edited = cargo_manifest(&package).expect("parsed");
    assert_eq!(edited["package"]["name"].as_str(), Some("bb"));
}
