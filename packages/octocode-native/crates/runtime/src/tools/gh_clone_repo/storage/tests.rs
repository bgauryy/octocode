use super::*;

#[test]
fn checkout_destinations_ignore_case() {
    let home = Path::new("/tmp/octocode-clones-root");
    let sha = "ABCDEF0123456789ABCDEF0123456789ABCDEF01";
    let entry =
        |owner, repo, commit| clone_dir(home, owner, repo, commit, None, 1, "https://example.test");
    let lower = sha.to_ascii_lowercase();
    assert_eq!(entry("o", "r", sha), entry("o", "r", &lower));
    // Owner/repo are case-insensitive on GitHub: one checkout.
    assert_eq!(entry("Octo", "Repo", sha), entry("octo", "repo", sha));
    assert!(entry("o", "r", sha).starts_with(home.join("clones/o/r")));
}
#[test]
fn deep_history_and_sparse_sets_key_their_own_entries() {
    let home = Path::new("/tmp/octocode-clones-root");
    let sha = "c".repeat(40);
    let shallow = clone_dir(home, "o", "r", &sha, None, 1, "https://example.test");
    let deep = clone_dir(home, "o", "r", &sha, None, 5, "https://example.test");
    assert_ne!(shallow, deep);
    assert!(deep.to_string_lossy().contains("__d5__host_"));
    crate::tools::test_support::checks::assert_absent(
        &serde_json::json!(shallow.to_string_lossy()),
        "__d",
    );
}
