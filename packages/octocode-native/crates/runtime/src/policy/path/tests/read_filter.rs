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

/// Under a directory the walk already admitted only the entry's own name can
/// add a reason to withhold it, so the full path is checked only for a name
/// that can complete a credential location; the decisions stay the policy's.
#[cfg(unix)]
#[test]
fn a_walk_checks_only_the_new_name_under_an_admitted_directory() {
    let root = fixture();
    let mut entries = vec![
        ".config/gh/hosts.yml",
        ".config/gh",
        ".config",
        ".config/gcloud/x",
        ".ssh/id",
        "a/credentials/mod.rs",
        "a/credentials/secrets.json",
        "a/.git/config",
        "a/.env",
        "a/.env.local",
        "a/id_rsa",
        "a/id_rsa.bak",
        "a/server.pem",
        "a/ok.rs",
        "a/gh/ok.rs",
        "Library/Keychains/k",
        "b/Library/Keychains/k",
    ];
    for entry in &entries {
        let path = root.join(entry);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        if !path.exists() {
            std::fs::write(&path, "x").expect("file");
        }
    }
    entries.sort_unstable();
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.clone()),
        ..Default::default()
    })
    .expect("policy");
    // Top-down, as a walk meets them: each directory before its entries.
    let mut walked = vec![root.clone()];
    for entry in &entries {
        let path = root.join(entry);
        for ancestor in path.ancestors().collect::<Vec<_>>().into_iter().rev() {
            if ancestor.starts_with(&root) && !walked.iter().any(|seen| seen == ancestor) {
                walked.push(ancestor.to_path_buf());
            }
        }
    }
    let walk = policy.discovery_walk();
    for path in &walked {
        let kind = std::fs::symlink_metadata(path)
            .ok()
            .map(|meta| meta.file_type());
        assert_eq!(
            walk.permits_entry(path, kind),
            policy.permits_discovery(path),
            "{}",
            path.display()
        );
    }
    let full_checks = |files: usize| {
        let root = fixture();
        std::fs::create_dir(root.join("a")).expect("dir");
        for file in 0..files {
            std::fs::write(root.join(format!("a/f{file}.rs")), "x").expect("file");
        }
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.clone()),
            ..Default::default()
        })
        .expect("policy");
        let walk = policy.discovery_walk();
        let kind = |path: &std::path::Path| {
            Some(std::fs::symlink_metadata(path).expect("stat").file_type())
        };
        let before = crate::policy::discovery::full_checks();
        assert!(walk.permits_entry(&root, kind(&root)));
        assert!(walk.permits_entry(&root.join("a"), kind(&root.join("a"))));
        for file in 0..files {
            let path = root.join(format!("a/f{file}.rs"));
            assert!(walk.permits_entry(&path, kind(&path)));
        }
        crate::policy::discovery::full_checks() - before
    };
    assert_eq!(full_checks(2), full_checks(20));
}

/// A walk's decisions do not depend on how a path is spelled or which spelling
/// came first: a repeated separator, a `.` or a trailing separator reaches the
/// same decision as the policy's, for what it withholds and what it admits.
#[cfg(unix)]
#[test]
fn every_spelling_of_a_path_is_decided_as_the_policy_decides_it() {
    let root = fixture();
    let outside = fixture();
    std::fs::write(outside.join("secret.rs"), "x").expect("outside file");
    for file in ["a/c/lib.rs", "a/c/.env.local", "a/.ssh/id", "a/.git/config"] {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(&path, "x").expect("file");
    }
    std::os::unix::fs::symlink(outside.join("secret.rs"), root.join("a/c/escape.rs"))
        .expect("escaping link");
    std::os::unix::fs::symlink(root.join("a/c"), root.join("a/inner")).expect("inner link");
    let entries = [
        "a",
        "a/c",
        "a/c/lib.rs",
        "a/c/.env.local",
        "a/c/escape.rs",
        "a/inner",
        "a/inner/lib.rs",
        "a/.ssh",
        "a/.ssh/id",
        "a/.git",
        "a/.git/config",
    ];
    let base = root.display();
    let spellings = |entry: &str| {
        let mut spelled = vec![
            format!("{base}/{entry}"),
            format!("{base}//{entry}"),
            format!("{base}/./{entry}"),
            format!("{base}/{}", entry.replace('/', "//")),
            format!("{base}/{}", entry.replace('/', "/./")),
        ];
        if root.join(entry).is_dir() {
            spelled.push(format!("{base}/{entry}/"));
        }
        spelled
    };
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.clone()),
        ..Default::default()
    })
    .expect("policy");
    let plain_first = entries.iter().flat_map(|entry| spellings(entry));
    let odd_first = entries
        .iter()
        .flat_map(|entry| spellings(entry).into_iter().rev());
    for order in [
        plain_first.collect::<Vec<_>>(),
        odd_first.collect::<Vec<_>>(),
    ] {
        let walk = policy.discovery_walk();
        let filter = DiscoveryWalk::owned(policy.clone());
        for spelled in &order {
            let path = std::path::Path::new(spelled);
            let kind = std::fs::symlink_metadata(path).ok().map(|m| m.file_type());
            assert_eq!(
                walk.permits_entry(path, kind),
                policy.permits_discovery(path),
                "{spelled}"
            );
            let read = if kind.is_some_and(|kind| kind.is_dir()) {
                policy.validate(path).is_ok()
            } else {
                policy.validate_read(path).is_ok()
            };
            assert_eq!(filter.allows_entry(path, kind), read, "{spelled} read");
        }
    }
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(outside);
}

/// A root spelled with a trailing separator seeds its entries like any other:
/// the walk canonicalizes only the root's parent.
#[cfg(unix)]
#[test]
fn a_root_with_a_trailing_separator_seeds_its_entries() {
    let root = fixture();
    std::fs::create_dir(root.join("a")).expect("dir");
    std::fs::write(root.join("a/lib.rs"), "x").expect("file");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.clone()),
        ..Default::default()
    })
    .expect("policy");
    let walk = policy.discovery_walk();
    let kind =
        |path: &std::path::Path| Some(std::fs::symlink_metadata(path).expect("stat").file_type());
    let spelled = std::path::PathBuf::from(format!("{}/", root.display()));
    let before = walk::CANONICALIZED.with(std::cell::Cell::get);
    assert!(walk.permits_entry(&spelled, kind(&spelled)));
    for entry in ["a", "a/lib.rs"] {
        let path = spelled.join(entry);
        assert!(walk.permits_entry(&path, kind(&path)), "{entry}");
    }
    let canonicalized = walk::CANONICALIZED.with(std::cell::Cell::get) - before;
    assert_eq!(canonicalized, 1, "only the walk root's parent");
    let _ = std::fs::remove_dir_all(root);
}
