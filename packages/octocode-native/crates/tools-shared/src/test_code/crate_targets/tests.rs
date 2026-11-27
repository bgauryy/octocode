use super::*;

fn targets(manifest: &str, listed: &[(&str, bool)]) -> CrateTargets {
    let manifest = toml::from_str::<Value>(manifest).expect("manifest");
    let listing = |folder: &Path| {
        listed
            .iter()
            .filter(|(entry, _)| Path::new(entry).parent() == folder.strip_prefix("/c").ok())
            .map(|(entry, is_dir)| (Path::new("/c").join(entry), *is_dir))
            .collect::<Vec<_>>()
    };
    CrateTargets::of(Path::new("/c"), &manifest, &listing)
}

const LISTED: &[(&str, bool)] = &[
    ("src/bin/a.rs", false),
    ("src/bin/b", true),
    ("src/bin/note.md", false),
    ("tests/it.rs", false),
];

#[test]
fn discovery_follows_the_manifest_and_its_edition() {
    let found = targets("[package]\nname = \"c\"\nedition = \"2021\"\n", LISTED);
    assert!(found.production.contains(Path::new("/c/src/bin/a.rs")));
    assert!(found.production.contains(Path::new("/c/src/bin/b/main.rs")));
    assert!(!found.production.contains(Path::new("/c/src/bin/note.md")));
    assert!(found.tests.contains(Path::new("/c/tests/it.rs")));
    let off = targets(
        "[package]\nname = \"c\"\nedition = \"2021\"\nautobins = false\n",
        LISTED,
    );
    assert!(!off.production.contains(Path::new("/c/src/bin/a.rs")));
    // Edition 2015 lists or discovers, not both.
    let old = targets("[package]\nname = \"c\"\n[[bin]]\nname = \"x\"\n", LISTED);
    assert!(old.production.contains(Path::new("/c/src/bin/x.rs")));
    assert!(!old.production.contains(Path::new("/c/src/bin/a.rs")));
}

#[test]
fn a_workspace_manifest_has_no_targets() {
    let none = targets("[workspace]\nmembers = [\"a\"]\n", LISTED);
    assert!(none.production.is_empty() && none.tests.is_empty());
}
