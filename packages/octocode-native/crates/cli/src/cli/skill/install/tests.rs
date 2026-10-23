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
