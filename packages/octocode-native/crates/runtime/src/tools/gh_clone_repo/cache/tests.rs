//! Clone cache bookkeeping: metadata, locks, stages and promotion.
use super::*;

#[test]
fn clone_cache_rejects_malformed_timestamps() {
    for bad in ["2026-01-01T00x00y00z000Z", "2026-02-31T00:00:00.000Z"] {
        let sha = "a".repeat(40);
        let mut meta = CacheMeta::new(
            &identity("o", "r", &sha, None, 1),
            &sha,
            Duration::from_secs(60),
            CheckoutFacts::default(),
        );
        meta.cloned_at = bad.into();
        assert!(
            CacheAge::of(&meta, Duration::from_secs(60)).is_none(),
            "{bad}"
        );
    }
}

#[test]
fn timestamp_round_trip() {
    for value in [0, 1_700_000_000_123, 1_900_000_000_999] {
        assert_eq!(rfc3339_millis(&iso8601_millis(value)), Some(value));
    }
    assert_eq!(rfc3339_millis("2024-01-01T00:00:00.00éZ"), None);
}

#[test]
fn commit_cache_keys_ignore_case() {
    let home = Path::new("/tmp/octocode-cache-root");
    let sha = "ABCDEF0123456789ABCDEF0123456789ABCDEF01";
    let entry =
        |owner, repo, commit| clone_dir(home, owner, repo, commit, None, 1, "https://example.test");
    let lower = sha.to_ascii_lowercase();
    assert_eq!(entry("o", "r", sha), entry("o", "r", &lower));
    // Owner/repo are case-insensitive on GitHub: one checkout.
    assert_eq!(entry("Octo", "Repo", sha), entry("octo", "repo", sha));
    assert!(entry("o", "r", sha).starts_with(home.join("tmp/clone/o/r")));
}

fn identity<'a>(
    owner: &'a str,
    repo: &'a str,
    commit: &'a str,
    sparse_key: Option<&'a str>,
    depth: u64,
) -> Identity<'a> {
    Identity {
        owner,
        repo,
        commit,
        sparse_key,
        depth,
    }
}

#[test]
fn cache_meta_matches_only_the_requested_identity() {
    let (a, b) = ("a".repeat(40), "b".repeat(40));
    let meta = CacheMeta::new(
        &identity("Owner", "Repo", &a, Some("src"), 1),
        &a,
        Duration::from_secs(60),
        CheckoutFacts::default(),
    );
    assert!(meta.matches(&identity("owner", "repo", &a, Some("src"), 1)));
    assert!(meta.matches(&identity("owner", "repo", &"A".repeat(40), Some("src"), 1)));
    assert!(!meta.matches(&identity("owner", "other", &a, Some("src"), 1)));
    assert!(!meta.matches(&identity("owner", "repo", &b, Some("src"), 1)));
    assert!(!meta.matches(&identity("owner", "repo", &a, None, 1)));
    assert!(!meta.matches(&identity("owner", "repo", &a, Some("src"), 5)));
}

#[test]
fn deep_history_and_sparse_sets_key_their_own_entries() {
    let home = Path::new("/tmp/octocode-cache-root");
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
