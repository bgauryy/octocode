use super::*;
use std::time::Duration;

/// A watch of `root` and `sources` alone.
fn watch<'a>(root: &'a Path, sources: &'a [PathBuf]) -> Watch<'a> {
    Watch {
        root,
        sources,
        rules: &[],
        markers: &[],
    }
}

/// An absent rule file that appears, a present one that changes, and a
/// marker that appears or goes each drop the stored scan.
#[test]
fn external_rules_and_markers_keep_their_presence_and_stamps() {
    let outside = tempfile::tempdir().expect("outside");
    let root = outside.path().join("scope");
    std::fs::create_dir(&root).expect("scope");
    let present = outside.path().join(".ignore");
    std::fs::write(&present, "x/\n").expect("rule");
    let absent = outside.path().join(".gitignore");
    let marker = outside.path().join(".git");
    let later = SystemTime::now() + SourceStamp::SETTLE + Duration::from_secs(1);
    let rules = [present.clone(), absent.clone()];
    let markers = [marker.clone()];
    let stored = |memo: &ScanMemo<u32>| {
        memo.put(
            ("s".into(), "k".into()),
            Watch {
                root: &root,
                sources: &[],
                rules: &rules,
                markers: &markers,
            },
            later,
            Arc::new(1),
            1,
        );
        assert_eq!(memo.get("s", "k").as_deref(), Some(&1));
    };
    let memo = ScanMemo::<u32>::new();
    stored(&memo);
    std::fs::write(&absent, "y/\n").expect("new rule file");
    assert!(memo.get("s", "k").is_none(), "an absent rule file appeared");
    std::fs::remove_file(&absent).expect("remove");
    stored(&memo);
    std::thread::sleep(Duration::from_millis(20));
    std::fs::write(&present, "z/\n").expect("edit rule");
    assert!(memo.get("s", "k").is_none(), "a rule file changed");
    stored(&memo);
    std::fs::create_dir(&marker).expect("marker");
    assert!(memo.get("s", "k").is_none(), "a repository marker appeared");
}

#[test]
fn discovery_root_does_not_watch_entries_outside_the_scope() {
    let outside = tempfile::tempdir().expect("outside");
    let root = outside.path().join("scope");
    std::fs::create_dir(&root).expect("scope");
    let memo = ScanMemo::<u32>::new();
    let later = SystemTime::now() + SourceStamp::SETTLE + Duration::from_secs(1);
    memo.put(
        ("s".into(), "k".into()),
        watch(&root, std::slice::from_ref(&root)),
        later,
        Arc::new(1),
        1,
    );
    assert_eq!(memo.get("s", "k").as_deref(), Some(&1));
    std::fs::write(outside.path().join("unrelated.ts"), "unrelated();").expect("outside edit");
    assert_eq!(memo.get("s", "k").as_deref(), Some(&1));
    std::fs::write(root.join("new.ts"), "target();").expect("inside edit");
    assert!(memo.get("s", "k").is_none());
}

#[test]
fn a_stored_scan_is_served_until_a_stamped_entry_changes() {
    let root = tempfile::tempdir().expect("root");
    let file = root.path().join("a.ts");
    std::fs::write(&file, "console.log(1);").expect("file");
    let memo = ScanMemo::<u32>::new();
    let sources = [file.clone()];
    // Written just now: not settled before the scan started.
    memo.put(
        ("s".into(), "k".into()),
        watch(root.path(), &sources),
        SystemTime::now(),
        Arc::new(1),
        1,
    );
    assert!(memo.get("s", "k").is_none(), "a racy scan is never stored");
    // A scan that started after the settle window is stored and served.
    let later = SystemTime::now() + SourceStamp::SETTLE + Duration::from_secs(1);
    memo.put(
        ("s".into(), "k".into()),
        watch(root.path(), &sources),
        later,
        Arc::new(2),
        1,
    );
    assert_eq!(memo.get("s", "k").as_deref(), Some(&2));
    assert!(memo.get("s", "other inputs").is_none());
    assert!(
        memo.put_over_budget_is_skipped(root.path(), &sources, later),
        "an over-budget scan is not stored"
    );
    // A sibling created after the scan changes the directory's stamp.
    std::fs::write(root.path().join("b.ts"), "console.log(2);").expect("newcomer");
    assert!(memo.get("s", "k").is_none());
    // The changed entry was evicted, not only skipped.
    assert!(memo.store.contents().0.is_empty());
}

/// A scan far larger than any per-store share, yet within the process
/// budget, is stored: refusing it would cost a whole scan on every later
/// page.
#[test]
fn one_scan_may_use_a_large_share_of_the_budget() {
    let dir = tempfile::tempdir().expect("fixture");
    let source = dir.path().join("a.ts");
    std::fs::write(&source, "x").expect("fixture");
    let memo = ScanMemo::<u32>::new();
    memo.put(
        ("whole".into(), "k".into()),
        watch(dir.path(), std::slice::from_ref(&source)),
        SystemTime::now()
            + octocode_engine::stamp::SourceStamp::SETTLE
            + std::time::Duration::from_secs(1),
        Arc::new(3),
        octocode_engine::memory::MEMORY.limit() / 4,
    );
    assert!(memo.get("whole", "k").is_some());
}

impl ScanMemo<u32> {
    fn put_over_budget_is_skipped(
        &self,
        root: &Path,
        sources: &[PathBuf],
        started: SystemTime,
    ) -> bool {
        self.put(
            ("big".into(), "k".into()),
            watch(root, sources),
            started,
            Arc::new(3),
            octocode_engine::memory::MEMORY.limit() + 1,
        );
        self.get("big", "k").is_none()
    }
}
