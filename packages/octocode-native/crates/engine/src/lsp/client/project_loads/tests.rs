use super::*;

fn key(name: &str) -> ProjectKey {
    Some(PathBuf::from(name))
}

#[test]
fn a_project_settles_once_its_first_open_waited() {
    let mut loads = ProjectLoads::default();
    assert!(!loads.start(&key("/p"), 0), "a new project waits");
    loads.finish(key("/p"), true, 1);
    assert!(loads.start(&key("/p"), 1), "its later files do not");
    assert!(!loads.start(&key("/q"), 1), "another project still waits");
}

#[test]
fn a_wait_that_timed_out_settles_nothing() {
    let mut loads = ProjectLoads::default();
    assert!(!loads.start(&None, 0));
    loads.finish(None, false, 1);
    assert!(!loads.start(&None, 1));
}

#[test]
fn progress_no_open_waited_for_forgets_every_project() {
    let mut loads = ProjectLoads::default();
    assert!(!loads.start(&key("/p"), 0));
    loads.finish(key("/p"), true, 1);
    assert!(!loads.start(&key("/p"), 2), "a later begin is unaccounted");
    loads.finish(key("/p"), true, 2);
    assert!(loads.start(&key("/p"), 2));
}

#[test]
fn progress_while_an_open_waits_is_that_opens() {
    let mut loads = ProjectLoads::default();
    assert!(!loads.start(&key("/p"), 0));
    loads.finish(key("/p"), true, 1);
    assert!(!loads.start(&key("/q"), 1));
    // A settled project's open started before /q's load began.
    assert!(loads.start(&key("/p"), 1));
    loads.finish(key("/q"), true, 3);
    assert!(loads.start(&key("/p"), 3));
    assert!(loads.start(&key("/q"), 3));
}

#[test]
fn clear_forgets_every_project() {
    let mut loads = ProjectLoads::default();
    assert!(!loads.start(&key("/p"), 0));
    loads.finish(key("/p"), true, 0);
    loads.clear();
    assert!(!loads.start(&key("/p"), 0));
}

#[test]
fn a_cancelled_wait_leaves_the_project_unsettled_and_stops_waiting() {
    let loads = StdMutex::new(ProjectLoads::default());
    let progress = ProgressTracker::new(crate::lsp::clock::LspClock::default());
    let pending = PendingLoad::start(&loads, &progress, key("/p"), 0).expect("waits");
    drop(pending);
    let state = loads.lock().expect("loads");
    assert_eq!(state.waiting, 0);
    assert!(!state.settled.contains(&key("/p")));
}

#[test]
fn project_key_is_the_nearest_typescript_config_directory() {
    let tmp = crate::test_support::TmpDir::new();
    let nested = tmp.0.join("pkg").join("src");
    std::fs::create_dir_all(&nested).expect("dirs");
    std::fs::write(tmp.0.join("pkg").join("tsconfig.json"), "{}").expect("config");
    let file = nested.join("a.ts").to_string_lossy().into_owned();
    assert_eq!(project_key(&file), Some(tmp.0.join("pkg")));
    let rust = nested.join("a.rs").to_string_lossy().into_owned();
    assert_eq!(project_key(&rust), None, "only TypeScript-family files");
}
