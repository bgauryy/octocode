//! One request's held proofs: a path one held proof stamped costs another
//! proof no stat, a lookup compares against it, a change still fails, and
//! nothing changes outside an observation.
use super::super::{Link, Proof, stamp_stats};
use super::*;
use std::path::PathBuf;

fn file(dir: &Path, name: &str, text: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, text).expect("write");
    path
}

fn stamp(path: &Path, link: Link) -> SourceStamp {
    let meta = match link {
        Link::Itself => std::fs::symlink_metadata(path),
        Link::Follow => std::fs::metadata(path),
    }
    .expect("meta");
    super::super::cache_stamp(&meta).expect("stamp")
}

/// A second proof over paths a held one stamped reads nothing, whether it
/// stamps them as themselves or through links, and a lookup of either kind
/// is proven by it.
#[test]
fn a_held_proof_answers_the_requests_later_checks() {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = vec![
        dir.path().to_path_buf(),
        file(dir.path(), "a.rs", "fn a() {}"),
    ];
    let first = Proof::capture(paths.clone(), Link::Itself, None).expect("stamped");
    let later = Proof::capture(paths.clone(), Link::Follow, None).expect("stamped");
    let observed = Arc::new(Observed::default());
    let before = stamp_stats();
    observing(Some(Arc::clone(&observed)), || {
        assert!(first.holds());
        assert!(later.holds());
        assert!(later.holds());
        let held = held();
        assert!(held.proves(&paths[1], Link::Follow, stamp(&paths[1], Link::Follow)));
        assert!(held.proves(&paths[1], Link::Itself, stamp(&paths[1], Link::Itself)));
    });
    assert_eq!(stamp_stats() - before, 2, "the first proof's two stats");
    assert_eq!(observed.stats(), 2);
    assert_eq!(
        observed.held_proofs(),
        2,
        "a proof checked twice is held once"
    );
}

/// Outside an observation every check reads, as before.
#[test]
fn outside_an_observation_each_check_reads() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = file(dir.path(), "a.rs", "x");
    let proof = Proof::capture([path.clone()], Link::Follow, None).expect("stamped");
    let before = stamp_stats();
    assert!(proof.holds() && proof.holds());
    assert_eq!(stamp_stats() - before, 2);
    assert!(!held().proves(&path, Link::Follow, stamp(&path, Link::Follow)));
}

/// A stamp other than the held one is read from the file system: a proof
/// kept from before an edit still fails, inside the observation too.
#[test]
fn a_different_stamp_is_read_and_a_change_fails() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = file(dir.path(), "a.rs", "x");
    let old = Proof::capture([path.clone()], Link::Follow, None).expect("stamped");
    std::fs::write(&path, "changed").expect("edit");
    let current = Proof::capture([path.clone()], Link::Itself, None).expect("stamped");
    observing(Some(Arc::new(Observed::default())), || {
        assert!(current.holds());
        assert!(!old.holds(), "the held proof shows another stamp");
        let other = SourceStamp {
            size: 1,
            ..stamp(&path, Link::Follow)
        };
        assert!(!held().proves(&path, Link::Follow, other));
    });
}

/// A failed check joins nothing: its paths prove nothing to later checks.
#[test]
fn a_failed_check_is_not_held() {
    let dir = tempfile::tempdir().expect("tempdir");
    let kept = file(dir.path(), "a.rs", "x");
    let gone = file(dir.path(), "b.rs", "x");
    let proof = Proof::capture([kept.clone(), gone.clone()], Link::Itself, None).expect("stamped");
    std::fs::remove_file(&gone).expect("remove");
    let observed = Arc::new(Observed::default());
    observing(Some(Arc::clone(&observed)), || {
        assert!(!proof.holds());
        assert!(!held().proves(&kept, Link::Itself, stamp(&kept, Link::Itself)));
    });
    assert_eq!(observed.held_proofs(), 0);
}

/// A stamp read through a link answers only reads through links: a symlink
/// stamped so proves its target's stamp, never the link itself.
#[cfg(unix)]
#[test]
fn a_followed_stamp_never_answers_for_the_link_itself() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = file(dir.path(), "target.rs", "fn t() {}");
    let link = dir.path().join("link.rs");
    std::os::unix::fs::symlink(&target, &link).expect("symlink");
    let through = Proof::capture([link.clone()], Link::Follow, None).expect("stamped");
    observing(Some(Arc::new(Observed::default())), || {
        assert!(through.holds());
        let held = held();
        assert!(held.proves(&link, Link::Follow, stamp(&target, Link::Itself)));
        assert!(!held.proves(&link, Link::Itself, stamp(&target, Link::Itself)));
        assert!(!held.proves(&link, Link::Itself, stamp(&link, Link::Itself)));
    });
}

/// Absent and present paths of a held proof answer presence checks.
#[test]
fn held_absence_and_presence_answer_later_checks() {
    let dir = tempfile::tempdir().expect("tempdir");
    let absent = dir.path().join("absent");
    let named = file(dir.path(), "named", "x");
    let mut first = Proof::default();
    first.link(absent.clone());
    first.link(named.clone());
    let mut later = Proof::default();
    later.follow(absent.clone());
    later.presence(named.clone());
    later.presence(absent.clone());
    observing(Some(Arc::new(Observed::default())), || {
        assert!(first.holds());
        let before = stamp_stats();
        assert!(later.holds());
        assert_eq!(stamp_stats() - before, 0);
    });
}

/// A large held proof, checked on several threads, answers a second large
/// proof with no stat.
#[test]
fn a_large_held_proof_answers_a_large_check() {
    let dir = tempfile::tempdir().expect("tempdir");
    let files = (0..2048)
        .map(|index| file(dir.path(), &format!("{index:05}.rs"), "x"))
        .collect::<Vec<_>>();
    let first = Proof::capture(files.clone(), Link::Itself, None).expect("stamped");
    let mut reversed = files.clone();
    reversed.reverse();
    let later = Proof::capture(reversed, Link::Follow, None).expect("stamped");
    let observed = Arc::new(Observed::default());
    observing(Some(Arc::clone(&observed)), || {
        assert!(first.holds());
        assert!(later.holds());
    });
    assert_eq!(observed.stats(), files.len());
}

/// The observation in force is the innermost, and leaving it restores the
/// one before.
#[test]
fn observations_nest_and_restore() {
    assert!(observation().is_none());
    let outer = Arc::new(Observed::default());
    observing(Some(Arc::clone(&outer)), || {
        let inner = Arc::new(Observed::default());
        observing(Some(Arc::clone(&inner)), || {
            assert!(observation().is_some_and(|now| Arc::ptr_eq(&now, &inner)));
        });
        observing(None, || assert!(observation().is_none()));
        assert!(observation().is_some_and(|now| Arc::ptr_eq(&now, &outer)));
    });
    assert!(observation().is_none());
}

/// A kept proof checked on every lookup of a request joins it once: the
/// held set stays as small as the request's distinct proofs, so a snapshot
/// costs one clone and a comparison a few lookups, however many checks ran
/// (callers of a common name check a kept verdict proof per caller).
#[test]
fn a_proof_checked_often_is_held_once() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = file(dir.path(), "a.rs", "x");
    let kept = Proof::capture([path.clone()], Link::Itself, None).expect("stamped");
    let observed = Arc::new(Observed::default());
    observing(Some(Arc::clone(&observed)), || {
        for _ in 0..500 {
            assert!(kept.clone().holds());
        }
        assert!(held().proves(&path, Link::Follow, stamp(&path, Link::Follow)));
    });
    assert_eq!(observed.held_proofs(), 1);
    assert_eq!(observed.stats(), 1, "the first check alone read");
}
