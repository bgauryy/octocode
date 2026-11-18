use super::*;

#[test]
fn every_embedded_skill_is_valid_and_round_trips() {
    assert!(!SKILLS.is_empty());
    for skill in SKILLS {
        assert_eq!(declared_name(skill.text()).expect(skill.name), skill.name);
        assert!(!skill.description.trim().is_empty(), "{}", skill.name);
        assert!(
            skill.text().lines().any(|line| {
                line.strip_prefix("description:")
                    .and_then(|value| serde_json::from_str::<String>(value.trim()).ok())
                    .is_some_and(|decoded| decoded == skill.description)
            }),
            "{} embeds a description its frontmatter does not hold",
            skill.name
        );
        let directory = tempfile::tempdir().expect("tempdir");
        skill.write(directory.path()).expect("extract");
        assert!(skill.matches(directory.path()), "{}", skill.name);
        fs::write(directory.path().join("unexpected.txt"), "user file").expect("write");
        assert!(!skill.matches(directory.path()));
    }
}

#[test]
fn declared_name_reads_one_plain_or_double_quoted_line() {
    let read = |header: &str| declared_name(&format!("---\n{header}\n---\nbody"));
    assert_eq!(
        read("name: demo-skill\ndescription: \"x\""),
        Ok("demo-skill".into())
    );
    assert_eq!(
        read("description: \"x\"\nname:   \"demo-2\"  "),
        Ok("demo-2".into())
    );
    for header in [
        "description: \"x\"",
        "name: |-\n  demo",
        "name: 'demo'",
        "name: demo # note",
        "name: \"demo",
        "name: \"de\\u006do\"",
        "name: demo\nname: other",
        "name:",
    ] {
        assert_eq!(read(header), Err(NAME_FORM.into()), "{header}");
    }
    assert!(declared_name("name: demo").is_err());
    assert!(declared_name("---\nname: demo\n").is_err());
}

#[test]
fn freshness_detects_same_size_edits_and_ignores_finder_metadata() {
    let skill = &SKILLS[0];
    let directory = tempfile::tempdir().expect("tempdir");
    skill.write(directory.path()).expect("write");
    fs::write(directory.path().join(".DS_Store"), "finder").expect("metadata");
    assert_eq!(skill.freshness(directory.path()), Some("fresh"));
    assert!(
        !skill.matches(directory.path()),
        "installation equality protects extra files"
    );
    let path = directory.path().join("SKILL.md");
    let original = fs::read(&path).expect("read");
    let mut edited = original.clone();
    edited[0] ^= 1;
    fs::write(&path, edited).expect("edit");
    assert_eq!(skill.freshness(directory.path()), Some("stale"));
    fs::write(path, original).expect("restore");
    assert_eq!(skill.freshness(directory.path()), Some("fresh"));
    fs::remove_file(directory.path().join("SKILL.md")).expect("remove");
    assert_eq!(skill.freshness(directory.path()), Some("stale"));
}

#[cfg(unix)]
#[test]
fn cyclic_or_missing_trees_have_unknown_freshness() {
    let skill = &SKILLS[0];
    let directory = tempfile::tempdir().expect("tempdir");
    skill.write(directory.path()).expect("write");
    std::os::unix::fs::symlink(directory.path(), directory.path().join("cycle")).expect("cycle");
    assert_eq!(skill.freshness(directory.path()), None);
    assert_eq!(skill.freshness(&directory.path().join("missing")), None);
}
