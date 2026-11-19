//! Identical `symbols` rows asked at once share one walk and one page, and
//! each answer is the one a lone request gives (perf P13, wave 3 `s7`).
use octocode_runtime_base::test_support::Fixture;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex, PoisonError};

/// What a test does when a row leads the page of its fixture's directory:
/// it is handed the count of rows waiting on the leader.
type Hook = Arc<dyn Fn(&dyn Fn() -> usize) + Send + Sync>;

/// Hooks by the query path they lead (each test owns its fixture's path).
static HOOKS: Mutex<Vec<(String, Hook)>> = Mutex::new(Vec::new());

/// Runs the hook a test set for a row leading the page of `path`.
pub(super) fn on_lead(path: &str, waiting: &dyn Fn() -> usize) {
    let hook = HOOKS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .find(|(hooked, _)| hooked == path)
        .map(|(_, hook)| Arc::clone(hook));
    if let Some(hook) = hook {
        hook(waiting);
    }
}

fn hook(path: &Value, hook: Hook) {
    HOOKS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push((path.as_str().expect("path").to_owned(), hook));
}

fn fixture() -> Fixture {
    let root = Fixture::new();
    for index in 0..6 {
        root.write(
            &format!("src/m{index}.rs"),
            format!("pub fn f{index}() {{}}\npub struct S{index};\n"),
        );
    }
    root.write("src/deep/inner.rs", "pub fn inner() {}\n");
    root
}

/// How many outline walks ran under `root`.
fn walks(root: &std::path::Path) -> usize {
    let root = root.canonicalize().expect("canonical root");
    crate::ast_search::symbols::WALKS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter(|walked| walked.starts_with(&root))
        .count()
}

/// `rows` copies of `row` asked at once, each on its own thread with the
/// caller's settled clock.
fn batch(
    row: &Value,
    rows: usize,
    paths: &octocode_runtime_base::policy::path::PathPolicy,
    security: &octocode_runtime_base::security::ContentSecurity,
) -> Vec<Value> {
    let through = octocode_engine::stamp::settled_through();
    std::thread::scope(|scope| {
        let tasks = (0..rows)
            .map(|_| {
                scope.spawn(move || {
                    octocode_engine::stamp::with_settled_through(through, || {
                        crate::ast_search::test_rows::execute_row(
                            row.clone(),
                            paths,
                            security,
                            &octocode_runtime_base::execution::cancel::NeverCancel,
                        )
                        .expect("outline")
                    })
                })
            })
            .collect::<Vec<_>>();
        tasks
            .into_iter()
            .map(|task| task.join().expect("row"))
            .collect()
    })
}

#[test]
fn identical_rows_asked_at_once_walk_once_and_answer_as_a_lone_row() {
    let root = fixture();
    octocode_engine::stamp::settle_written_so_far();
    let (paths, security) = octocode_runtime_base::test_support::simple_policy(&root.0);
    let row = json!({"operation":"symbols","path":root.0.join("src"),"limit":2});
    let lone = batch(&row, 1, &paths, &security).remove(0);
    // A lone row's leader waits on nothing: it hands nothing on.
    assert!(
        lone["files"]
            .as_array()
            .is_some_and(|files| !files.is_empty()),
        "{lone}"
    );
    assert_eq!(walks(&root.0), 1);
    // The leader waits until the other three rows wait on it.
    hook(
        &row["path"],
        Arc::new(|waiting: &dyn Fn() -> usize| {
            while waiting() < 3 {
                std::thread::yield_now();
            }
        }),
    );
    let answers = batch(&row, 4, &paths, &security);
    for answer in &answers {
        assert_eq!(answer, &lone);
    }
    // One more walk: the leader's. The three rows that waited took its page.
    assert_eq!(walks(&root.0), 2);
}

#[test]
fn a_page_built_while_its_files_change_is_handed_to_no_row() {
    let root = fixture();
    octocode_engine::stamp::settle_written_so_far();
    let (paths, security) = octocode_runtime_base::test_support::simple_policy(&root.0);
    let row = json!({"operation":"symbols","path":root.0.join("src")});
    let written = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let file = root.0.join("src/m0.rs");
    hook(&row["path"], {
        let written = Arc::clone(&written);
        Arc::new(move |waiting: &dyn Fn() -> usize| {
            // Only the leader comes here first; the rows that waited on it
            // come here as they compute for themselves.
            if written.load(std::sync::atomic::Ordering::SeqCst) {
                return;
            }
            while waiting() < 2 {
                std::thread::yield_now();
            }
            // Written once the leader started: its page proves nothing.
            std::fs::write(&file, "pub fn changed() {}\n").expect("edit");
            written.store(true, std::sync::atomic::Ordering::SeqCst);
        })
    });
    let answers = batch(&row, 3, &paths, &security);
    // Every row walked and outlined for itself, and each lists the edit.
    assert_eq!(walks(&root.0), 3);
    for answer in &answers {
        assert!(answer.to_string().contains("changed"), "{answer}");
    }
}

/// What proves a shared page covers each entry its walk could have listed:
/// a file it listed, an admitted directory (a file added there), an ignore
/// file in the scope and one above it.
#[test]
fn the_proof_of_a_page_watches_its_walk_and_its_ignore_rules() {
    let root = fixture();
    root.write(".gitignore", "*.log\n");
    octocode_engine::stamp::settle_written_so_far();
    let (paths, _) = octocode_runtime_base::test_support::simple_policy(&root.0);
    let proof = || {
        let q: octocode_contract_types::AstSearchQuerySymbols =
            serde_json::from_value(octocode_runtime_base::test_support::opened(
                octocode_runtime_base::id::ToolId::AstSearch,
                &json!({"operation":"symbols","path":root.0.join("src")}),
            ))
            .expect("typed symbols row");
        crate::ast_search::symbols::execute_symbols_proven(
            &q,
            &paths,
            &octocode_runtime_base::execution::cancel::NeverCancel,
            None,
            &|| true,
        )
        .expect("outline")
        .2
        .expect("a whole, settled outline is proven")
    };
    let edits: [(&str, &dyn Fn()); 4] = [
        ("a listed file", &|| {
            root.write("src/m1.rs", "pub fn m1() {}\n");
        }),
        ("a file in an admitted directory", &|| {
            root.write("src/deep/added.rs", "pub fn added() {}\n");
        }),
        ("an ignore file in the scope", &|| {
            root.write("src/.ignore", "*.tmp\n");
        }),
        ("the ignore file above the scope", &|| {
            root.write(".gitignore", "*.log\n*.tmp\n");
        }),
    ];
    for (what, edit) in edits {
        let proof = proof();
        assert!(proof.holds(), "{what}: holds before the edit");
        edit();
        assert!(!proof.holds(), "{what}: an edit breaks the proof");
        octocode_engine::stamp::settle_written_so_far();
    }
}
