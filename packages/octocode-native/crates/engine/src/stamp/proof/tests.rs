//! The one proof: stamped paths, presence, the settle window and large sets.
use super::*;

/// One proof on every platform: a stamp is captured (not refused for a
/// missing change time), stands while nothing changes, and falls with an
/// edit, a sibling's arrival in a stamped directory, or a deletion.
#[test]
fn a_proof_holds_until_one_path_changes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("a.rs");
    std::fs::write(&file, "fn a() {}").expect("write");
    let paths = || [dir.path().to_path_buf(), file.clone()];
    let proof = Proof::capture(paths(), Link::Itself, None).expect("stamped");
    assert!(proof.holds());
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(dir.path().join("b.rs"), "fn b() {}").expect("sibling");
    assert!(!proof.holds(), "the directory changed");
    let proof = Proof::capture(paths(), Link::Follow, None).expect("stamped");
    std::fs::remove_file(&file).expect("remove");
    assert!(!proof.holds(), "a deleted path is a change");
    assert!(Proof::capture(paths(), Link::Follow, None).is_none());
}

/// A set large enough to be stamped and checked on several threads
/// keeps every path once, in order, and still falls with one change or
/// one missing path anywhere in it.
#[test]
fn a_large_proof_is_one_across_its_chunks() {
    let dir = tempfile::tempdir().expect("tempdir");
    let files = (0..STATS_PER_WORKER * STAT_WORKERS + 7)
        .map(|index| {
            let file = dir.path().join(format!("{index:05}.rs"));
            std::fs::write(&file, "fn a() {}").expect("write");
            file
        })
        .collect::<Vec<_>>();
    let proof = Proof::capture(files.clone(), Link::Follow, None).expect("stamped");
    assert_eq!(
        proof
            .seen
            .iter()
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>(),
        files
    );
    assert!(proof.holds());
    let last = files.last().expect("files");
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(last, "fn b() {}").expect("edit");
    assert!(!proof.holds(), "the last chunk's edit");
    std::fs::remove_file(&files[0]).expect("remove");
    assert!(Proof::capture(files, Link::Follow, None).is_none());
}

#[test]
fn a_capture_needs_settled_paths_when_asked() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("a.rs");
    std::fs::write(&file, "fn a() {}").expect("write");
    let stamp = SourceStamp::of(&std::fs::metadata(&file).expect("meta")).expect("stamp");
    let written = std::time::UNIX_EPOCH + std::time::Duration::from_nanos(stamp.last_write_ns());
    assert!(Proof::capture([file.clone()], Link::Follow, Some(written)).is_none());
    let later = written + stamp.settle_for();
    assert!(Proof::capture([file], Link::Follow, Some(later)).is_some());
}

/// A path added absent stays proven while it stays absent, and falls once it
/// appears; one added by presence ignores edits and follows only its name.
#[test]
fn an_absent_path_and_a_presence_are_proven_too() {
    let dir = tempfile::tempdir().expect("tempdir");
    let absent = dir.path().join("absent");
    let mut proof = Proof::default();
    proof.follow(absent.clone());
    proof.link(dir.path().join("also-absent"));
    assert!(proof.holds());
    std::fs::write(&absent, "x").expect("write");
    assert!(!proof.holds(), "an absent path appeared");

    let named = dir.path().join("named");
    std::fs::write(&named, "x").expect("write");
    let mut proof = Proof::default();
    proof.presence(named.clone());
    assert!(proof.holds());
    std::fs::write(&named, "xx").expect("edit");
    assert!(proof.holds(), "presence ignores an edit");
    std::fs::remove_file(&named).expect("remove");
    assert!(!proof.holds(), "the name is gone");
}

/// The settle check reads stamps only: a presence or an absent path never
/// counts as written.
#[test]
fn settled_before_asks_only_what_was_stamped() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("a.rs");
    std::fs::write(&file, "x").expect("write");
    let mut proof = Proof::default();
    proof.follow(dir.path().join("absent"));
    proof.presence(file.clone());
    let stamp = SourceStamp::of(&std::fs::metadata(&file).expect("meta")).expect("stamp");
    let written = std::time::UNIX_EPOCH + std::time::Duration::from_nanos(stamp.last_write_ns());
    assert!(proof.settled_before(written));
    proof.follow(file);
    assert!(!proof.settled_before(written), "written at the instant");
    let window = stamp.settle_for();
    assert!(!proof.settled_before(written + window - std::time::Duration::from_nanos(1)));
    assert!(proof.settled_before(written + window));
}

/// Stamps a walk took stand as captured ones, so they cost no stat, and a
/// presence joins the same check.
#[test]
fn taken_stamps_and_presences_join_one_proof() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("a.rs");
    std::fs::write(&file, "x").expect("write");
    let stamp = SourceStamp::of(&std::fs::symlink_metadata(&file).expect("meta")).expect("stamp");
    let marker = dir.path().join("marker");
    let proof = Proof::default()
        .with_taken(Link::Itself, vec![(file.clone(), stamp)])
        .with_presence(vec![(marker.clone(), false)]);
    assert_eq!(proof.stamps_read(), 0, "taken stamps cost no stat");
    assert!(proof.holds());
    std::fs::write(&marker, "x").expect("marker");
    assert!(!proof.holds(), "the marker appeared");
}

/// A proof says what it read: each stamp it took itself while it was built,
/// each stamp it compares when checked, and none past the first change.
#[test]
fn a_proof_reports_the_stamps_it_read() {
    let dir = tempfile::tempdir().expect("tempdir");
    let files = ["a.rs", "b.rs", "c.rs"].map(|name| dir.path().join(name));
    for file in &files {
        std::fs::write(file, "x").expect("write");
    }
    let mut built = Proof::capture(files.clone(), Link::Follow, None).expect("all exist");
    assert_eq!(built.stamps_read(), files.len());
    built.follow(dir.path().join("absent"));
    assert_eq!(
        built.stamps_read(),
        files.len() + 1,
        "an absent path is read too"
    );
    let joined = Proof::default().with(&built);
    assert_eq!(
        joined.stamps_read(),
        built.stamps_read(),
        "joined, its reads come along"
    );
    assert_eq!(
        built.check(),
        Checked {
            holds: true,
            stamps_read: files.len() + 1
        }
    );
    std::fs::remove_file(&files[0]).expect("remove");
    let changed = built.check();
    assert!(!changed.holds, "a deleted path is a change");
    assert_eq!(
        changed.stamps_read, 1,
        "the check stops at the first change"
    );
}

/// A capture with a walk's stamps stats only the other paths, and holds the
/// taken stamps to the capture's rules: one written within its settle window
/// before the capture's instant, or one that proves nothing, refuses it as a
/// stat would. The taken path stays watched: an edit fails the proof.
#[test]
fn a_capture_with_taken_stamps_stats_only_the_rest() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("a.rs");
    std::fs::write(&file, "x").expect("write");
    let stamp = SourceStamp::of(&std::fs::symlink_metadata(&file).expect("meta")).expect("stamp");
    let written = std::time::UNIX_EPOCH + std::time::Duration::from_nanos(stamp.last_write_ns());
    let settled = written + stamp.settle_for();
    let capture = |taken: SourceStamp, settled_before| {
        Proof::capture_with_taken(
            [dir.path().to_path_buf()],
            vec![(file.clone(), taken)],
            Link::Itself,
            settled_before,
        )
    };
    let before = stamp_stats_of(&file);
    let proof = capture(stamp, Some(settled)).expect("settled");
    assert_eq!(
        stamp_stats_of(&file),
        before,
        "the taken file is not stat'ed"
    );
    assert_eq!(proof.stamps_read(), 1, "the directory alone");
    assert!(proof.holds());
    assert!(
        capture(stamp, Some(written)).is_none(),
        "written at the capture's instant"
    );
    assert!(capture(stamp, None).is_some(), "no settle check asked");
    let unprovable = SourceStamp {
        changed_ns: 0,
        ..stamp
    };
    if cfg!(unix) {
        assert!(
            capture(unprovable, None).is_none(),
            "no change time proves nothing"
        );
    }
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&file, "y").expect("edit");
    assert!(!proof.holds(), "the taken file is watched");
}
