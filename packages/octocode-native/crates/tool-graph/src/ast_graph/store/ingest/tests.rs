//! Ingests run concurrently: only staging, publication and retention take
//! the workspace build lock.
use super::hooks::{self, Point};
use super::*;
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_runtime_base::test_support::{Fixture, workspace_policy};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, mpsc};

mod resumed_builds;

/// A repository holding two scopes, `a/` and `b/`.
fn workspace() -> Fixture {
    let dir = Fixture::new();
    std::fs::create_dir_all(dir.0.join(".git")).expect("git marker");
    for scope in ["a", "b"] {
        crate::ast_graph::test_support::write_under(
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

#[test]
fn completed_ingests_keep_extraction_artifacts_as_the_scopes_fact_cache() {
    let dir = workspace();
    let root = dir.0.to_path_buf();
    let job = super::super::super::graph::checkpoint::job_path(
        &root.join(".octocode/graph/.facts"),
        &root.join("a").canonicalize().unwrap(),
    );
    // Both builds record the same stamps however long the first one took.
    octocode_engine::stamp::settle_written_so_far();
    let first = run(&root, "a", &NeverCancel);
    assert_eq!(first.class, ExitClass::Success, "{}", first.value);
    assert!(job.exists(), "completed extraction stays as the fact cache");
    let unchanged = run(&root, "a", &NeverCancel);
    assert_eq!(
        unchanged.value["extraction"]["parsedFiles"],
        unchanged.value["scan"]["filesScanned"]
    );
    assert_eq!(unchanged.value["extraction"]["reusedFiles"], 0);
    assert_eq!(unchanged.value["sha256"], first.value["sha256"]);
    assert!(job.exists());
    dir.write("a/lib.ts", "export function f() { return 42; }\n");
    let changed = run(&root, "a", &NeverCancel);
    assert_eq!(changed.class, ExitClass::Success, "{}", changed.value);
    let manifest: Value = serde_json::from_slice(
        &std::fs::read(Path::new(changed.value["dir"].as_str().unwrap()).join(MANIFEST_FILE))
            .unwrap(),
    )
    .unwrap();
    // The edited file is extracted; every other file comes from the cache.
    assert_eq!(manifest["extraction"]["parsedFiles"], 1);
    assert_eq!(
        manifest["extraction"]["reusedFiles"].as_u64().unwrap() + 1,
        manifest["scan"]["filesScanned"].as_u64().unwrap()
    );
    assert!(job.exists());
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

/// Two builds of one fresh scope both find no snapshot and both build. The
/// one that reaches publication second finds the first's snapshot of the
/// unchanged tree and answers with it: one snapshot, one id.
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
        let work = &output.value["extraction"];
        assert_eq!(
            work["parsedFiles"].as_u64().unwrap() + work["reusedFiles"].as_u64().unwrap(),
            output.value["scan"]["filesScanned"].as_u64().unwrap()
        );
    }
    assert_eq!(id(&outputs[0]), id(&outputs[1]));
    for key in ["sha256", "counts", "calls", "scan", "root", "dir"] {
        assert_eq!(outputs[0].value[key], outputs[1].value[key], "{key}");
    }
    let home = root.join(".octocode/graph");
    assert_eq!(list_snapshots(&home), [id(&outputs[0])]);
    assert_eq!(
        std::fs::read_to_string(home.join(LATEST_FILE)).expect("latest"),
        id(&outputs[0])
    );
    let job = super::super::super::graph::checkpoint::job_path(
        &home.join(".facts"),
        &root.join("a").canonicalize().expect("scope"),
    );
    // Whichever build finished last owns the job: it is gone or it is the
    // scope's settled fact cache, never unfinished work.
    if let Ok(record) = std::fs::read_to_string(job.join("job.json")) {
        let record: Value = serde_json::from_str(&record).expect("job record");
        assert_eq!(record["stage"], "settled", "{record}");
    }
    // The scope is unchanged since: the next build answers with it too.
    let again = run(&root, "a", &NeverCancel);
    assert_eq!(id(&again), id(&outputs[0]), "{}", again.value);
}

/// A snapshot published while a build ran answers that build only when it
/// is what the build would publish: `refresh` always publishes its own, and
/// so does a build of a tree edited since that snapshot.
#[test]
fn a_build_publishes_its_own_snapshot_when_the_one_published_meanwhile_differs() {
    for refresh in [true, false] {
        let dir = workspace();
        let root = dir.0.to_path_buf();
        let (other_root, edited) = (root.clone(), dir.0.join("a/lib.ts"));
        let meanwhile = Rc::new(RefCell::new(None));
        let slot = Rc::clone(&meanwhile);
        hooks::set(move |point, _| {
            if point != Point::Publish || slot.borrow().is_some() {
                return;
            }
            let root = other_root.clone();
            let other = std::thread::spawn(move || run(&root, "a", &NeverCancel));
            *slot.borrow_mut() = Some(other.join().expect("the other ingest"));
            if !refresh {
                std::fs::write(&edited, "export function f() { return 42; }\n").expect("edit");
            }
        });
        let options = IngestOptions {
            path: root.join("a"),
            workspace: Some(root.clone()),
            refresh,
            ..IngestOptions::default()
        };
        let own = ingest(
            &options,
            &workspace_policy(&root),
            &ContentSecurity::new(),
            &NeverCancel,
            None,
        );
        hooks::set(|_, _| {});
        let other = meanwhile.borrow_mut().take().expect("publish point");
        assert_eq!(own.class, ExitClass::Success, "{}", own.value);
        assert_eq!(other.class, ExitClass::Success, "{}", other.value);
        assert_ne!(id(&own), id(&other), "refresh {refresh}");
        let home = root.join(".octocode/graph");
        assert_eq!(list_snapshots(&home), [id(&other), id(&own)]);
    }
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

/// A build of an unchanged scope answers with its current snapshot and
/// builds nothing; an edit, other options or `refresh` build again.
#[test]
fn an_unchanged_scope_reuses_its_current_snapshot() {
    let dir = workspace();
    let root = dir.0.to_path_buf();
    let builds = Rc::new(AtomicUsize::new(0));
    let counted = Rc::clone(&builds);
    hooks::set(move |point, _| {
        if point == Point::Build {
            counted.fetch_add(1, Ordering::Relaxed);
        }
    });
    let ingest_with = |options: IngestOptions| {
        ingest(
            &options,
            &workspace_policy(&root),
            &ContentSecurity::new(),
            &NeverCancel,
            None,
        )
    };
    let options = IngestOptions {
        path: root.join("a").canonicalize().expect("scope"),
        workspace: Some(root.clone()),
        ..IngestOptions::default()
    };
    let first = ingest_with(options.clone());
    assert_eq!(first.class, ExitClass::Success, "{}", first.value);
    let again = ingest_with(options.clone());
    assert_eq!(again.class, ExitClass::Success, "{}", again.value);
    assert_eq!(id(&again), id(&first), "{}", again.value);
    assert_eq!(
        builds.load(Ordering::Relaxed),
        1,
        "the unchanged scope was rebuilt"
    );
    for key in ["counts", "calls", "linkage", "scan", "sha256", "root"] {
        assert_eq!(again.value[key], first.value[key], "{key}");
    }
    let refreshed = ingest_with(IngestOptions {
        refresh: true,
        ..options.clone()
    });
    assert_ne!(id(&refreshed), id(&first), "refresh always rebuilds");
    let excluded = ingest_with(IngestOptions {
        exclude: vec!["lib.ts".into()],
        ..options.clone()
    });
    assert_ne!(id(&excluded), id(&refreshed), "other options build again");
    let back = ingest_with(options.clone());
    assert_ne!(
        id(&back),
        id(&excluded),
        "the newest snapshot was built with other options"
    );
    dir.write("a/lib.ts", "export function f() { return 42; }\n");
    let edited = ingest_with(options);
    assert_ne!(id(&edited), id(&back), "an edit builds again");
    assert_eq!(builds.load(Ordering::Relaxed), 5);
    assert_eq!(
        std::fs::read_to_string(root.join(".octocode/graph").join(LATEST_FILE)).expect("latest"),
        id(&edited)
    );
}

/// A source or linking config edited while the graph was built does not
/// lose the build: the snapshot publishes what the build read, its warning
/// counts the edited files and names graphQuery stale, which lists each one
/// with its config flag, and the related same graphBuild links the new
/// content, parsing only the edited sources again.
#[test]
fn an_edit_during_the_build_publishes_what_was_read_and_marks_it_stale() {
    let dir = workspace();
    let root = dir.0.to_path_buf();
    dir.write("a/tsconfig.json", "{}");
    octocode_engine::stamp::settle_written_so_far();
    let edits = [
        (
            root.join("a/lib.ts"),
            "export function f() { return 42; }\n",
        ),
        (root.join("a/tsconfig.json"), "{\"compilerOptions\":{}}"),
    ];
    let edited = Rc::new(std::cell::Cell::new(false));
    let once = Rc::clone(&edited);
    hooks::set(move |point, _| {
        if point == Point::Settle && !once.replace(true) {
            for (path, text) in &edits {
                std::fs::write(path, text).expect("edit");
            }
        }
    });
    let built = run(&root, "a", &NeverCancel);
    hooks::set(|_, _| {});
    assert!(edited.get(), "the settle point was reached");
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    let warnings = built.value["warnings"].to_string();
    assert!(
        warnings.contains("2 files changed") && warnings.contains("graphQuery stale"),
        "{}",
        built.value
    );
    let related = built.value["related"].as_array().expect("related");
    let (stale, rebuild) = (
        related.iter().find(|call| call["tool"] == "graphQuery"),
        related.iter().find(|call| call["tool"] == "graphBuild"),
    );
    let stale = &stale.expect("stale lead")["input"]["queries"][0];
    assert_eq!(stale["operation"], "stale", "{stale}");
    assert_eq!(stale["graph"], built.value["id"], "{stale}");
    assert_eq!(
        rebuild.expect("rebuild lead")["input"]["queries"][0]["path"],
        "a"
    );

    let rows = query(
        &QueryOptions {
            op: "stale".into(),
            graph: built.value["id"].as_str().map(str::to_owned),
            workspace: Some(root.clone()),
            ..QueryOptions::default()
        },
        &workspace_policy(&root),
    );
    let mut changed = rows.value["rows"]
        .as_array()
        .expect("stale rows")
        .iter()
        .map(|row| {
            (
                row["path"].clone(),
                row["status"].clone(),
                row["config"].clone(),
            )
        })
        .collect::<Vec<_>>();
    changed.sort_by_key(|row| row.0.to_string());
    assert_eq!(
        changed,
        [
            (json!("a/lib.ts"), json!("changed"), Value::Null),
            (json!("a/tsconfig.json"), json!("changed"), json!(true)),
        ],
        "{}",
        rows.value
    );

    let again = run(&root, "a", &NeverCancel);
    assert_eq!(again.class, ExitClass::Success, "{}", again.value);
    assert_ne!(id(&again), id(&built));
    assert!(again.value.get("warnings").is_none(), "{}", again.value);
    assert_eq!(
        again.value["extraction"]["parsedFiles"], 1,
        "{}",
        again.value
    );
}
