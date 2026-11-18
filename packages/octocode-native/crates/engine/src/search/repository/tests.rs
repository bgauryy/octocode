use super::*;

#[test]
fn a_git_or_jujutsu_marker_makes_a_repository_root() {
    let base = tempfile::tempdir().expect("tempdir");
    let nested = base.path().join("a/b");
    std::fs::create_dir_all(&nested).expect("dirs");
    assert!(!is_root(base.path()));
    assert_eq!(nearest(&nested), None);
    for marker in MARKERS {
        let checkout = base.path().join(marker.trim_start_matches('.'));
        let inner = checkout.join("src");
        std::fs::create_dir_all(&inner).expect("dirs");
        std::fs::create_dir(checkout.join(marker)).expect("marker");
        assert!(is_root(&checkout), "{marker}");
        assert_eq!(nearest(&inner), Some(checkout.as_path()), "{marker}");
    }
    // A worktree's `.git` is a file naming its git directory.
    std::fs::write(base.path().join("a/.git"), "gitdir: ../x\n").expect("gitfile");
    assert_eq!(nearest(&nested), Some(base.path().join("a").as_path()));
}
