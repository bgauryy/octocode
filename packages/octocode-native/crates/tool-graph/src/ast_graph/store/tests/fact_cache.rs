//! A published build keeps its files' extraction as the scope's fact cache:
//! an edit extracts the edited file alone, and the snapshot it publishes is
//! the one a cold build publishes.
use super::*;

fn graph_bytes(dir: &Path, build: &GraphOutput) -> Vec<u8> {
    let id = build.value["graph"].as_str().expect("snapshot id");
    std::fs::read(dir.join(".octocode/graph").join(id).join("graph.bin")).expect("graph.bin")
}

fn jobs(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir.join(".octocode/graph/.facts/jobs"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect()
}

fn facts_in(job: &Path) -> usize {
    std::fs::read_dir(job)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "fact"))
        .count()
}

/// Ages every source past the stamp window, so each build records the same
/// stamps whatever time the test spent between its builds (a stamp is kept
/// only of a file untouched for [`octocode_engine::stamp::SourceStamp::SETTLE`]).
fn settle_sources(dir: &Path) {
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    for entry in std::fs::read_dir(dir.join("app/src"))
        .expect("sources")
        .flatten()
    {
        std::fs::File::options()
            .write(true)
            .open(entry.path())
            .and_then(|file| file.set_modified(old))
            .expect("age");
    }
    // Aging a file moves its change time to now: settle that too, on this
    // thread's clock, rather than wait out the window.
    octocode_engine::stamp::settle_written_so_far();
}

/// T2: after one edit the published graph bytes equal a cold build's, with
/// a file that decoded from Latin-1 reused unchanged.
#[test]
fn an_edit_publishes_the_snapshot_a_cold_build_publishes() {
    let dir = fixture();
    let root = dir.0.as_path();
    std::fs::write(
        root.join("app/src/latin.ts"),
        b"// caf\xe9\nexport const latin = () => 1;\n",
    )
    .expect("latin-1 source");
    settle_sources(root);
    let first = ingest_fixture(root, None);
    assert_eq!(first.class, ExitClass::Success, "{}", first.value);
    dir.write("app/src/a.ts", "export const a = 5;\n");
    settle_sources(root);
    let warm = ingest_fixture(root, None);
    assert_eq!(warm.class, ExitClass::Success, "{}", warm.value);
    let scanned = warm.value["scan"]["filesScanned"].as_u64().expect("files");
    assert_eq!(warm.value["extraction"]["parsedFiles"], 1, "{}", warm.value);
    assert_eq!(
        warm.value["extraction"]["reusedFiles"].as_u64(),
        Some(scanned - 1),
        "{}",
        warm.value
    );
    assert_eq!(warm.value["extraction"]["resumedFiles"], 0);
    let cold = ingest_root(root, &root.join("app"), None, true);
    assert_eq!(cold.class, ExitClass::Success, "{}", cold.value);
    assert_eq!(cold.value["extraction"]["reusedFiles"], 0);
    assert!(
        graph_bytes(root, &warm) == graph_bytes(root, &cold),
        "the warm snapshot's graph.bin differs from the cold one's"
    );
    for field in ["counts", "calls", "linkage", "scan", "argumentCoverage"] {
        assert_eq!(warm.value[field], cold.value[field], "{field}");
    }
}

/// T8: a published build leaves its artifacts as a settled cache, and the
/// receipt's unfinished bytes name only jobs a build could resume.
#[test]
fn a_published_build_keeps_a_settled_cache_and_names_no_unfinished_bytes() {
    let dir = fixture();
    let out = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(out.class, ExitClass::Success, "{}", out.value);
    assert!(
        out.value.get("unfinishedJobBytes").is_none(),
        "{}",
        out.value
    );
    let jobs = jobs(dir.0.as_path());
    assert_eq!(jobs.len(), 1, "{jobs:?}");
    assert_eq!(
        facts_in(&jobs[0]),
        out.value["scan"]["filesScanned"].as_u64().unwrap() as usize
    );
}
