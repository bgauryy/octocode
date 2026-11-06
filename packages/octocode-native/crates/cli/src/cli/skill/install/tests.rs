use super::platforms::SKILL_PLATFORMS;
use super::*;

#[test]
fn failed_publication_restores_previous_content_and_cleans_staging() {
    let root = tempfile::tempdir().expect("root");
    let destination = root.path().join("skill");
    fs::create_dir(&destination).expect("directory");
    fs::write(destination.join("SKILL.md"), "previous").expect("write");
    // No staged path forces the publish rename to fail after the backup exists.
    assert!(replace_with(&destination, |_| Ok(())).is_err());
    assert_eq!(
        fs::read_to_string(destination.join("SKILL.md")).expect("restored"),
        "previous"
    );
    assert_eq!(fs::read_dir(root.path()).expect("list").count(), 1);
}

#[test]
fn platform_parser_deduplicates_and_validates_every_token() {
    assert_eq!(
        parse_skill_platforms(" Claude,claude,CODEX ").expect("platforms"),
        vec![SkillPlatform::Claude, SkillPlatform::Codex]
    );
    assert_eq!(
        parse_skill_platforms("codex,all").expect("all").len(),
        SKILL_PLATFORMS.len()
    );
    for invalid in [
        "all,typo",
        "agents",
        "shared",
        "claude-desktop",
        "codex-native",
    ] {
        assert!(parse_skill_platforms(invalid).is_err(), "{invalid}");
    }
}

#[cfg(unix)]
#[test]
fn symlinked_skill_manifest_is_rejected_before_install() {
    let root = tempfile::tempdir().expect("root");
    fs::write(root.path().join("actual.md"), "instructions").expect("write");
    std::os::unix::fs::symlink("actual.md", root.path().join("SKILL.md")).expect("link");
    assert!(
        validate_skill("valid", &root.path().to_string_lossy())
            .is_some_and(|error| error.contains("regular file"))
    );
}

/// Two trees are the same only when every entry matches in kind and
/// content: links by target, files by bytes, directories by their names
/// and each entry below.
#[cfg(unix)]
#[test]
fn same_tree_compares_kind_and_content_of_every_entry() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().expect("dir");
    let at = |name: &str| dir.path().join(name);
    for side in ["left", "right"] {
        fs::create_dir_all(at(side).join("nested")).expect("tree");
        fs::write(at(side).join("nested/file"), "same").expect("file");
        symlink("nested/file", at(side).join("link")).expect("link");
    }
    assert!(same_tree(&at("left"), &at("right")));
    assert!(!same_tree(&at("left"), &at("missing")));

    fs::write(at("right").join("nested/file"), "changed").expect("edit");
    assert!(!same_tree(&at("left"), &at("right")), "different bytes");
    fs::write(at("right").join("nested/file"), "same").expect("restore");
    fs::write(at("right").join("extra"), "x").expect("extra");
    assert!(!same_tree(&at("left"), &at("right")), "an extra entry");
    fs::remove_file(at("right").join("extra")).expect("remove extra");

    fs::remove_file(at("right").join("link")).expect("unlink");
    symlink("elsewhere", at("right").join("link")).expect("other link");
    assert!(
        !same_tree(&at("left"), &at("right")),
        "a link to another target"
    );
    fs::remove_file(at("right").join("link")).expect("unlink");
    fs::write(at("right").join("link"), "file").expect("file in place of link");
    assert!(
        !same_tree(&at("left"), &at("right")),
        "a file in place of a link"
    );
    assert!(
        !same_tree(&at("left/nested/file"), &at("right/nested")),
        "a file and a directory"
    );
}

/// A copied tree keeps a link as a link to the same target.
#[cfg(unix)]
#[test]
fn copy_tree_keeps_links_as_links() {
    let dir = tempfile::tempdir().expect("dir");
    let source = dir.path().join("source");
    fs::create_dir_all(&source).expect("source");
    fs::write(source.join("file"), "bytes").expect("file");
    std::os::unix::fs::symlink("file", source.join("link")).expect("link");
    let copy = dir.path().join("copy");
    tree::copy_tree(&source, &copy).expect("copy");
    assert_eq!(
        fs::read_link(copy.join("link")).expect("link copied"),
        Path::new("file")
    );
    assert!(same_tree(&source, &copy));
}

/// A destination below missing directories resolves through its nearest
/// existing ancestor, so an alias of the source is still recognised.
#[test]
fn a_missing_destination_resolves_through_its_existing_ancestor() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("canonical");
    let missing = dir.path().join("a/b/c");
    assert_eq!(physical_destination(&missing), root.join("a/b/c"));
}

/// A managed link points to its target whether it was written absolute or
/// relative; a regular file or a missing path points nowhere.
#[cfg(unix)]
#[test]
fn points_to_reads_absolute_and_relative_links() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("canonical");
    let target = root.join("skill");
    fs::create_dir_all(&target).expect("target");
    let absolute = root.join("absolute");
    std::os::unix::fs::symlink(&target, &absolute).expect("absolute link");
    let relative = root.join("relative");
    std::os::unix::fs::symlink("skill", &relative).expect("relative link");
    let target = target.to_string_lossy();
    assert!(points_to(&absolute, &target));
    assert!(points_to(&relative, &target));
    assert!(!points_to(&relative, &root.join("other").to_string_lossy()));
    assert!(!points_to(&root.join("skill"), &target), "not a link");
    assert!(!points_to(&root.join("missing"), &target));
}
