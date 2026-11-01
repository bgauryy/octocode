//! The text search read filter ([`DiscoveryWalk`] as a
//! `TextSearchPathFilter`): the read policy's decisions, at one
//! canonicalization per walk instead of one per file.
use super::*;
use octocode_engine::search::text_search::TextSearchPathFilter;

/// Every entry of a top-down walk is decided as the read policy decides
/// it (`validate` for a directory, `validate_read` for a file): links that
/// escape the roots stay denied, links that stay inside are resolved, a
/// sensitive file and a credential directory are withheld, and a file that
/// is not regular is never read. Only the walk root's parent is
/// canonicalized: the policy alone canonicalizes every entry.
#[cfg(unix)]
#[test]
fn the_read_filter_decides_as_the_read_policy_with_one_canonicalization() {
    let root = fixture();
    let outside = fixture();
    std::fs::write(outside.join("secret.rs"), "x").expect("outside file");
    std::fs::create_dir_all(outside.join("tree")).expect("outside dir");
    let mut entries = Vec::new();
    for top in ["a", "b"] {
        for mid in ["c", "d"] {
            let dir = format!("{top}/{mid}");
            std::fs::create_dir_all(root.join(&dir)).expect("dirs");
            for file in ["lib.rs", "main.rs", ".env.local"] {
                std::fs::write(root.join(&dir).join(file), "x").expect("file");
                entries.push(format!("{dir}/{file}"));
            }
            entries.extend([top.to_owned(), dir]);
        }
    }
    std::fs::create_dir_all(root.join("a/.ssh")).expect("credential dir");
    std::os::unix::fs::symlink(outside.join("secret.rs"), root.join("a/c/escape.rs"))
        .expect("escaping file link");
    std::os::unix::fs::symlink(outside.join("tree"), root.join("a/c/escape_dir"))
        .expect("escaping dir link");
    std::os::unix::fs::symlink(root.join("b/c/lib.rs"), root.join("a/inside.rs"))
        .expect("inner link");
    crate::tools::test_support::make_fifo(&root.join("a/d/pipe.rs"));
    entries.extend([
        "a/.ssh".into(),
        "a/c/escape.rs".into(),
        "a/c/escape_dir".into(),
        "a/inside.rs".into(),
        "a/d/pipe.rs".into(),
    ]);
    entries.sort();
    entries.dedup();
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.clone()),
        ..Default::default()
    })
    .expect("policy");
    let filter = DiscoveryWalk::owned(policy.clone());
    let before = walk::CANONICALIZED.with(std::cell::Cell::get);
    for entry in &entries {
        let path = root.join(entry);
        let kind = std::fs::symlink_metadata(&path)
            .ok()
            .map(|meta| meta.file_type());
        let is_dir = kind.is_some_and(|kind| kind.is_dir());
        let expected = if is_dir {
            policy.validate(&path).is_ok()
        } else {
            policy.validate_read(&path).is_ok()
        };
        assert_eq!(filter.allows_entry(&path, kind), expected, "{entry}");
        assert_eq!(filter.allows(&path, is_dir), expected, "{entry} untyped");
    }
    let canonicalized = walk::CANONICALIZED.with(std::cell::Cell::get) - before;
    assert_eq!(canonicalized, 1, "only the walk root's parent");
    for denied in [
        "a/c/escape.rs",
        "a/c/escape_dir",
        "a/d/pipe.rs",
        "a/c/.env.local",
    ] {
        let path = root.join(denied);
        let kind = std::fs::symlink_metadata(&path)
            .ok()
            .map(|meta| meta.file_type());
        assert!(!filter.allows_entry(&path, kind), "{denied} is never read");
    }
    assert!(filter.allows_entry(&root.join("a/inside.rs"), None));
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(outside);
}
