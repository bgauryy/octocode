//! Ingests run concurrently: only staging, publication and retention take
//! the workspace build lock.
use super::hooks::{self, Point};
use super::*;
use crate::execution::cancel::NeverCancel;
use crate::tools::test_support::{Fixture, workspace_policy};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, mpsc};

/// A repository holding two scopes, `a/` and `b/`.
fn workspace() -> Fixture {
    let dir = Fixture::new();
    std::fs::create_dir_all(dir.0.join(".git")).expect("git marker");
    for scope in ["a", "b"] {
        crate::tools::ast_graph::test_support::write_under(
            &dir,
            scope,
            &[
                (
                    "main.ts",
                    "import { f } from './lib';\nexport const main = () => f();\n",
                ),
                (
                    "lib.ts",
                    "export function f() { return g(); }\nfunction g() { return 1; }\n",
                ),
            ],
        );
    }
    dir
}

fn run(dir: &Path, scope: &str, cancel: &dyn CancellationCheck) -> GraphOutput {
    let options = IngestOptions {
        path: dir.join(scope),
        workspace: Some(dir.to_path_buf()),
        ..IngestOptions::default()
    };
    ingest(
        &options,
        &workspace_policy(dir),
        &ContentSecurity::new(),
        cancel,
        None,
    )
}

/// Whether another holder could take the workspace build lock now.
fn build_lock_free(home: &Path) -> bool {
    std::fs::create_dir_all(home).expect("graph home");
    let file = super::super::super::graph::checkpoint::lock::open(&home.join(".build.lock"))
        .expect("lock file");
    file.try_lock().is_ok()
}

fn id(output: &GraphOutput) -> String {
    output.value["id"].as_str().expect("snapshot id").to_owned()
}

#[test]
fn ingests_of_different_scopes_build_while_no_lock_is_held() {
    let dir = workspace();
    let root = dir.0.to_path_buf();
    let (go, start) = mpsc::channel::<()>();
    let (finished, done) = mpsc::channel::<GraphOutput>();
    let other = {
        let root = root.clone();
        std::thread::spawn(move || {
            start.recv().expect("start");
            finished
                .send(run(&root, "b", &NeverCancel))
                .expect("report");
        })
    };
    // `a` pauses before its build; `b` scans, parses, links, projects and
    // publishes in that window, which a lock held across the build forbids.
    let seen = Rc::new(RefCell::new(None));
    let slot = Rc::clone(&seen);
    hooks::set(move |point, home| {
        if point != Point::Build || slot.borrow().is_some() {
            return;
        }
        let free = build_lock_free(home);
        go.send(()).expect("start the other ingest");
        let other = free.then(|| done.recv().expect("the other ingest's receipt"));
        *slot.borrow_mut() = Some((free, other));
    });
    let first = run(&root, "a", &NeverCancel);
    other.join().expect("other ingest");
    let (free, second) = seen
        .borrow_mut()
        .take()
        .expect("the build point was reached");
    assert!(free, "no build lock is held while a scope builds");
    let second = second.expect("the other scope finished during this build");
    assert_eq!(first.class, ExitClass::Success, "{}", first.value);
    assert_eq!(second.class, ExitClass::Success, "{}", second.value);
    assert_ne!(id(&first), id(&second));
    assert!(second.value.get("reused").is_none(), "{}", second.value);
    let home = root.join(".octocode/graph");
    assert_eq!(
        std::fs::read_to_string(home.join(LATEST_FILE)).expect("latest"),
        id(&first),
        "the last publication is latest"
    );
    for output in [&first, &second] {
        assert!(home.join(id(output)).join(MANIFEST_FILE).is_file());
    }
}

#[test]
fn concurrent_ingests_of_one_scope_publish_one_snapshot() {
    let dir = workspace();
    let root = dir.0.to_path_buf();
    let both = Arc::new(Barrier::new(2));
    let runs = (0..2)
        .map(|_| {
            let root = root.clone();
            let both = Arc::clone(&both);
            std::thread::spawn(move || {
                hooks::set(move |point, _| {
                    if point == Point::Build {
                        both.wait();
                    }
                });
                run(&root, "a", &NeverCancel)
            })
        })
        .collect::<Vec<_>>();
    let outputs = runs
        .into_iter()
        .map(|run| run.join().expect("ingest"))
        .collect::<Vec<_>>();
    for output in &outputs {
        assert_eq!(output.class, ExitClass::Success, "{}", output.value);
    }
    assert_eq!(id(&outputs[0]), id(&outputs[1]), "one snapshot serves both");
    let reused = outputs
        .iter()
        .filter(|output| output.value["reused"] == true)
        .count();
    assert_eq!(reused, 1, "the later ingest reuses the earlier snapshot");
    let home = root.join(".octocode/graph");
    assert_eq!(
        std::fs::read_to_string(home.join(LATEST_FILE)).expect("latest"),
        id(&outputs[0])
    );
    assert_eq!(list_snapshots(&home), vec![id(&outputs[0])]);
    let job = super::super::super::graph::checkpoint::job_path(
        &home.join(".facts"),
        &root.join("a").canonicalize().expect("scope"),
    );
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(job.join("job.json")).expect("job")).expect("json");
    assert_eq!(manifest["stage"], "complete");
    let again = run(&root, "a", &NeverCancel);
    assert_eq!(again.value["reused"], true, "{}", again.value);
}

/// Counts the checks an ingest makes once armed, and cancels on request.
#[derive(Default)]
struct Witness {
    armed: AtomicBool,
    checks: AtomicUsize,
    cancel: AtomicBool,
}

impl CancellationCheck for Witness {
    fn check(&self) -> Result<(), String> {
        if self.armed.load(Ordering::SeqCst) {
            self.checks.fetch_add(1, Ordering::SeqCst);
        }
        if self.cancel.load(Ordering::SeqCst) {
            return Err("cancelled".into());
        }
        Ok(())
    }
}

#[test]
fn a_waiting_ingest_never_cleans_live_staging_and_its_wait_cancels() {
    let dir = workspace();
    let root = dir.0.canonicalize().expect("root");
    let home = root.join(".octocode/graph");
    let policy = workspace_policy(&root);
    std::fs::create_dir_all(&home).expect("home");
    let held = publication::lock(&home, &policy, &NeverCancel).expect("build lock");
    let staged = publication::Staged::new(&home).expect("live staging");
    std::fs::write(staged.graph(), b"partial").expect("staged graph");
    let witness = Arc::new(Witness::default());
    let waiting = {
        let root = root.clone();
        let witness = Arc::clone(&witness);
        std::thread::spawn(move || {
            let armed = Arc::clone(&witness);
            hooks::set(move |point, _| {
                if point == Point::Publish {
                    armed.armed.store(true, Ordering::SeqCst);
                }
            });
            run(&root, "b", &*witness)
        })
    };
    // After the publish point only the lock wait checks for cancellation:
    // two checks mean at least one attempt found the lock held.
    while witness.checks.load(Ordering::SeqCst) < 2 {
        assert!(!waiting.is_finished(), "the ingest waits for the held lock");
        std::thread::yield_now();
    }
    assert!(staged.graph().is_file(), "live staging survives the wait");
    witness.cancel.store(true, Ordering::SeqCst);
    let cancelled = waiting.join().expect("waiting ingest");
    assert_eq!(
        cancelled.value["errorCode"], "cancelled",
        "{}",
        cancelled.value
    );
    assert!(staged.graph().is_file(), "a cancelled wait cleans nothing");
    drop(staged);
    drop(held);
    let published = run(&root, "b", &NeverCancel);
    assert_eq!(published.class, ExitClass::Success, "{}", published.value);
}

/// A deadline that falls once `stop_at` extraction artifacts are committed.
struct Deadline {
    job: PathBuf,
    stop_at: usize,
}

fn artifacts(job: &Path) -> usize {
    std::fs::read_dir(job)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "fact"))
        .count()
}

impl CancellationCheck for Deadline {
    fn check(&self) -> Result<(), String> {
        if artifacts(&self.job) >= self.stop_at {
            return Err("deadline reached".into());
        }
        Ok(())
    }
}

/// A scope larger than one deadline's work completes across builds: each
/// stopped build keeps its committed extraction and says so, and the next
/// resumes from it until the whole scope publishes.
#[test]
fn a_scope_past_one_deadline_completes_across_builds() {
    let dir = Fixture::new();
    std::fs::create_dir_all(dir.0.join(".git")).expect("git marker");
    // Files run as wide as the parse pool, each checking the deadline
    // before it starts: a scope many times that width is cut mid-scope.
    let width = std::thread::available_parallelism().map_or(1, usize::from);
    let files = (0..(8 * width).max(32))
        .map(|id| {
            (
                format!("f{id:03}.ts"),
                format!("export function f{id}() {{ return {id}; }}\n"),
            )
        })
        .collect::<Vec<_>>();
    let files = files
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect::<Vec<_>>();
    crate::tools::ast_graph::test_support::write_under(&dir, "big", &files);
    let root = dir.0.canonicalize().expect("root");
    let job = super::super::super::graph::checkpoint::job_path(
        &root.join(".octocode/graph/.facts"),
        &root.join("big"),
    );
    let mut stopped = 0;
    let receipt = loop {
        let deadline = Deadline {
            stop_at: artifacts(&job) + 3,
            job: job.clone(),
        };
        let out = run(&root, "big", &deadline);
        if out.class == ExitClass::Success {
            break out;
        }
        assert_eq!(out.value["errorCode"], "cancelled", "{}", out.value);
        assert!(
            out.value["hints"]
                .as_array()
                .is_some_and(|hints| hints.iter().any(|hint| hint == RESUME_HINT)),
            "{}",
            out.value
        );
        stopped += 1;
        assert!(
            stopped < files.len(),
            "every stopped build commits progress"
        );
    };
    assert!(stopped >= 2, "the scope needed more than one deadline");
    assert_eq!(
        receipt.value["scan"]["filesScanned"],
        files.len(),
        "{}",
        receipt.value
    );
    assert!(
        receipt.value["extraction"]["resumedFiles"].as_u64() > Some(0),
        "{}",
        receipt.value
    );
}
