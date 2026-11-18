use super::*;

#[test]
fn a_missing_repository_warning_names_the_successor_when_known() {
    let mut artifact = ArtifactItem::new(ArtifactType::Npm, "pkg".into(), "https://r/pkg".into());
    artifact.version = Some("1.0.0".into());
    artifact.repository = Some("git://github.com/old/pkg.git".into());
    artifact.repository_status = Some(RepositoryStatus::Missing);
    let plain = source_warning(&artifact).expect("warning");
    assert!(!plain.contains("latest release"), "{plain}");
    artifact.successor_repository = Some("new/pkg".into());
    let named = source_warning(&artifact).expect("warning");
    assert!(named.contains("latest release names new/pkg"), "{named}");
}
