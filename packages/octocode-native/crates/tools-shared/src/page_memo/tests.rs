use super::*;
use std::time::Duration;

/// A walked directory's ignore inputs: its present regular ignore files,
/// a nested repository's rule files outside the walk (never the root's:
/// those are the root's external inputs), and a link, a non-file or a
/// withheld ignore file as a dependency no stamp watches.
#[cfg(unix)]
#[test]
fn a_walked_directory_names_its_ignore_inputs() {
    let root = tempfile::tempdir().expect("fixture");
    let root = root.path();
    std::fs::create_dir(root.join(".git")).expect("root repository");
    std::fs::write(root.join(".gitignore"), "*.log\n").expect("fixture");
    let all = |_: &Path| true;
    let inputs = directory_ignore_inputs_with(
        root,
        root,
        octocode_engine::search::walk::GlobalIgnore::NONE,
        all,
    );
    assert_eq!(
        inputs,
        DirectoryInputs {
            files: vec![root.join(".gitignore")],
            ..DirectoryInputs::default()
        }
    );
    let nested = root.join("nested");
    std::fs::create_dir_all(nested.join(".git")).expect("nested repository");
    std::fs::write(nested.join(".ignore"), "tmp\n").expect("fixture");
    let inputs = directory_ignore_inputs_with(
        &nested,
        root,
        octocode_engine::search::walk::GlobalIgnore::NONE,
        all,
    );
    assert_eq!(inputs.files, vec![nested.join(".ignore")]);
    let real = std::fs::canonicalize(&nested).expect("real path");
    assert!(
        inputs
            .external
            .contains(&real.join(".git").join("info").join("exclude")),
        "{:?}",
        inputs.external
    );
    assert!(!inputs.untracked);
    assert!(
        directory_ignore_inputs_with(
            &nested,
            root,
            octocode_engine::search::walk::GlobalIgnore::NONE,
            |_| false
        )
        .untracked
    );
    std::os::unix::fs::symlink(nested.join(".ignore"), nested.join(".gitignore")).expect("link");
    assert!(
        directory_ignore_inputs_with(
            &nested,
            root,
            octocode_engine::search::walk::GlobalIgnore::NONE,
            all
        )
        .untracked
    );
    std::fs::create_dir(root.join("dir")).expect("fixture");
    std::fs::create_dir(root.join("dir").join(".ignore")).expect("a directory");
    assert!(
        directory_ignore_inputs_with(
            &root.join("dir"),
            root,
            octocode_engine::search::walk::GlobalIgnore::NONE,
            all
        )
        .untracked
    );
}

/// A charge grows the stored value's weight; one past the whole budget
/// is refused and changes nothing, and nothing stored takes no charge.
#[test]
fn a_charge_grows_a_stored_value_within_the_budget() {
    let memo = PageMemo::new();
    memo.put("a".into(), "p".into(), 3, ());
    let stored = memo.contents().1;
    assert!(memo.charge("a", 5));
    assert_eq!(memo.contents().1, stored + 5);
    assert!(!memo.charge("a", octocode_engine::memory::MEMORY.limit() + 1));
    assert_eq!(memo.contents().1, stored + 5);
    assert!(!memo.charge("missing", 1));
}

/// A value over the whole budget is not kept, and evicts nothing
/// (oldest-first eviction across stores: `octocode_io::memo` tests).
#[test]
fn a_value_over_the_whole_budget_is_not_kept() {
    let memo = PageMemo::new();
    memo.put("small".into(), "p".into(), 3, ());
    memo.put(
        "huge".into(),
        "p".into(),
        octocode_engine::memory::MEMORY.limit(),
        (),
    );
    assert_eq!(memo.contents(), (vec!["small".into()], 3 + 5 + 1));
}

#[test]
fn a_value_is_served_under_its_own_policy_until_replaced_or_evicted() {
    let memo = PageMemo::new();
    memo.put("s".into(), "a".into(), 1, 1);
    assert_eq!(memo.get("s", "a", |v| *v), Some(1));
    assert_eq!(memo.get("s", "b", |v| *v), None);
    memo.put("s".into(), "a".into(), 1, 2);
    assert_eq!(memo.get("s", "a", |v| *v), Some(2));
    assert_eq!(memo.contents().0.len(), 1);
    memo.evict("s");
    assert_eq!(memo.get("s", "a", |v| *v), None);
}

/// A test forgets what its own workspace kept: the values of another policy,
/// a test beside it in the process, stay served.
#[test]
fn evicting_a_policy_keeps_every_other_policys_values() {
    let memo = PageMemo::new();
    memo.put("mine".into(), "a".into(), 1, 1);
    memo.put("also mine".into(), "a".into(), 1, 2);
    memo.put("theirs".into(), "b".into(), 1, 3);
    memo.evict_policy("a");
    assert_eq!(memo.get("mine", "a", |v| *v), None);
    assert_eq!(memo.get("also mine", "a", |v| *v), None);
    assert_eq!(memo.get("theirs", "b", |v| *v), Some(3));
}

#[test]
fn a_value_is_served_while_its_proof_holds_and_evicted_when_refused() {
    let root = tempfile::tempdir().expect("fixture");
    let marker = root.path().join("marker");
    let proof = || {
        Watch {
            root: root.path(),
            sources: &[],
            rules: &[],
            markers: std::slice::from_ref(&marker),
        }
        .capture(Link::Follow, None)
        .expect("proof")
    };
    let memo = PageMemo::new();
    let get = |serve: fn(u8) -> Option<u8>| {
        memo.get_current(
            "s",
            "a",
            |(value, proof): &(u8, Proof)| (*value, proof.clone()),
            serve,
        )
    };
    memo.put("s".into(), "a".into(), 1, (7, proof()));
    assert_eq!(get(Some), Some(7));
    // `serve` refusing the parts evicts them.
    assert_eq!(get(|_| None), None);
    assert!(memo.contents().0.is_empty());
    // A changed watched entry fails the proof: not served, and evicted.
    memo.put("s".into(), "a".into(), 1, (7, proof()));
    std::fs::write(&marker, "").expect("marker");
    assert_eq!(get(Some), None);
    assert!(memo.contents().0.is_empty());
}

/// `value` stored under policy `a` as if put `age` ago.
fn put_aged(memo: &PageMemo<()>, age: Duration) {
    let stored_at = std::time::Instant::now()
        .checked_sub(age)
        .expect("monotonic clock covers the age");
    memo.memo.insert_full(
        "s".into(),
        ("a".into(), ()),
        (),
        1,
        Some(CONTINUATION_IDLE),
        stored_at,
    );
}

#[test]
fn a_read_restarts_the_continuation_lifetime() {
    let memo = PageMemo::new();
    put_aged(&memo, CONTINUATION_IDLE - Duration::from_secs(5));
    assert_eq!(memo.get("s", "a", |()| ()), Some(()));
    let age = memo.memo.get("s").expect("still stored").age;
    assert!(
        age < Duration::from_secs(5),
        "read restarted the lifetime: {age:?}"
    );
    // A read under another policy is no use of this value.
    put_aged(&memo, CONTINUATION_IDLE - Duration::from_secs(5));
    assert_eq!(memo.get("s", "b", |()| ()), None);
    let age = memo.memo.get("s").expect("still stored").age;
    assert!(age >= CONTINUATION_IDLE - Duration::from_secs(5), "{age:?}");
}

#[test]
fn an_expired_value_is_not_served() {
    let memo = PageMemo::new();
    put_aged(&memo, CONTINUATION_IDLE - Duration::from_millis(20));
    std::thread::sleep(Duration::from_millis(40));
    assert_eq!(memo.get("s", "a", |()| ()), None);
}
