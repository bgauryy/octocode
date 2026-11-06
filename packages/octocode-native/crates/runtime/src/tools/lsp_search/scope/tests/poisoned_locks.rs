//! A panic contained elsewhere in the request poisons a lock. A record it then
//! dropped would read as coverage it never had, so records keep being made.
use super::*;

/// Poison `lock` the way a contained panic does.
fn poison<T: Send>(lock: &std::sync::Mutex<T>) {
    std::thread::scope(|threads| {
        let holder = threads.spawn(|| {
            let _held = lock
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            std::panic::resume_unwind(Box::new("contained panic"));
        });
        assert!(holder.join().is_err());
    });
    assert!(lock.is_poisoned());
}

#[test]
fn a_poisoned_answer_still_records_what_the_server_answered() {
    let scope = Scope::new("/w".into(), vec!["*.rs".into()]);
    poison(&scope.answered);
    scope.answer(["/w/a.rs"]);
    assert!(scope.is_answered("/w/a.rs"));
}

#[test]
fn a_poisoned_anchor_still_records_its_site_and_lines() {
    let scope = Scope::new("/w".into(), vec!["*.rs".into()]);
    poison(&scope.site);
    poison(&scope.listed);
    scope.anchor_site("/w/a.rs", 3, 7);
    scope.answer_lines(false, [("/w/a.rs", 3), ("/w/a.rs", 4)]);
    let site = scope
        .site
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert_eq!(site.as_ref().expect("the site").line, 3);
    let listed = scope
        .listed
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert!(
        listed
            .lines_of("/w/a.rs")
            .is_some_and(|lines| lines.contains(&4))
    );
}

/// The scan's files stay known: without them `text_only_files` has nothing to say.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_poisoned_scan_still_records_its_files() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("root");
    std::fs::write(root.join("a.rs"), "fn wanted() {}\n").expect("source");
    let scope = Scope::new(root.to_string_lossy().into_owned(), vec!["*.rs".into()]);
    poison(&scope.text);
    let policy = workspace_policy(&root);
    scope
        .text_files("wanted", &policy, &crate::execution::cancel::NeverCancel)
        .await
        .expect("scan");
    assert!(scope.scanned_files("wanted").is_some());
}
