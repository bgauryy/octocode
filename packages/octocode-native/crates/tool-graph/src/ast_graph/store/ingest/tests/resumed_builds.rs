//! Builds stopped by a deadline or an empty budget keep their committed
//! extraction, and a later build resumes from it.
use super::*;
use octocode_test_support::json_fixture;

/// A deadline that falls once `stop_at` extraction artifacts are committed.
/// A build checks it at every unit of work, so it lists the job directory only
/// when the directory changed since its last look (one `stat` instead of a
/// listing), and once fallen it stays fallen.
struct Deadline {
    job: PathBuf,
    stop_at: usize,
    fallen: std::sync::atomic::AtomicBool,
    listed_at: std::sync::Mutex<Option<std::time::SystemTime>>,
}

impl Deadline {
    fn new(job: PathBuf, stop_at: usize) -> Self {
        Self {
            job,
            stop_at,
            fallen: false.into(),
            listed_at: std::sync::Mutex::new(None),
        }
    }
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
        use std::sync::atomic::Ordering;
        if !self.fallen.load(Ordering::Relaxed) {
            let modified = std::fs::metadata(&self.job)
                .and_then(|metadata| metadata.modified())
                .ok();
            let mut listed_at = self
                .listed_at
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if modified.is_none() || *listed_at != modified {
                *listed_at = modified;
                if artifacts(&self.job) >= self.stop_at {
                    self.fallen.store(true, Ordering::Relaxed);
                }
            }
        }
        if self.fallen.load(Ordering::Relaxed) {
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
    crate::ast_graph::test_support::write_under(&dir, "big", &files);
    let root = dir.0.canonicalize().expect("root");
    let job = super::super::super::super::graph::checkpoint::job_path(
        &root.join(".octocode/graph/.facts"),
        &root.join("big"),
    );
    let mut stopped = 0;
    let receipt = loop {
        let deadline = Deadline::new(job.clone(), artifacts(&job) + 3);
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

/// A checkpointed build whose call has no budget left for extraction pauses
/// with a partial answer and its progress instead of being cut at the call's
/// deadline, and the next call builds the scope.
#[test]
fn a_build_out_of_budget_pauses_with_a_resumable_answer() {
    struct Spent(std::time::Instant);
    impl CancellationCheck for Spent {
        fn check(&self) -> Result<(), String> {
            Ok(())
        }
        fn deadline(&self) -> Option<std::time::Instant> {
            Some(self.0)
        }
    }
    let dir = workspace();
    let root = dir.0.to_path_buf();
    let paused = run(&root, "a", &Spent(std::time::Instant::now()));
    assert_eq!(paused.class, ExitClass::Success, "{}", paused.value);
    assert_eq!(paused.value["isPartial"], true, "{}", paused.value);
    assert_eq!(
        paused.value["partialReasons"],
        json_fixture(r#"["scanBudget"]"#)
    );
    let progress = &paused.value["progress"];
    assert_eq!(
        progress["committedFiles"].as_u64().unwrap() + progress["remainingFiles"].as_u64().unwrap(),
        2,
        "{}",
        paused.value
    );
    assert!(paused.value.get("graph").is_none() && paused.value.get("errorCode").is_none());
    let built = run(&root, "a", &NeverCancel);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
}
