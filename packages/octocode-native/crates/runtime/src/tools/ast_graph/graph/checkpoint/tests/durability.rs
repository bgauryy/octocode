//! Artifacts are written without a sync and verified on every read; facts
//! kept decoded link without reading their artifacts back.
use super::*;

fn artifact(job_dir: &Path, name: &str) -> PathBuf {
    job_dir.join(format!(
        "{}.fact",
        octocode_engine::digest::sha256(name.as_bytes())
    ))
}

/// A crash can leave an unsynced artifact torn (cut anywhere, even inside
/// its header) or a temporary behind: the torn artifact fails verification
/// and is extracted again, the temporary is removed, and the build equals a
/// clean one.
#[test]
fn torn_artifacts_and_leftover_temporaries_are_extracted_again() {
    let (_dir, mut scan) = fixture();
    build(&scan, &NeverCancel).unwrap();
    let job_dir = job_path(scan.checkpoint.as_ref().unwrap(), &scan.root);
    let whole = std::fs::read(artifact(&job_dir, "f02.ts")).unwrap();
    let cuts = [0, 30, 65, 66, whole.len() / 2, whole.len() - 1];
    for (file, cut) in ["f02.ts", "f03.ts", "f04.ts", "f05.ts", "f06.ts", "f07.ts"]
        .into_iter()
        .zip(cuts)
    {
        let path = artifact(&job_dir, file);
        let bytes = std::fs::read(&path).unwrap();
        std::fs::write(&path, &bytes[..cut.min(bytes.len() - 1)]).unwrap();
    }
    let temporary = job_dir.join(".job.json-00ff.tmp");
    std::fs::write(&temporary, b"{\"partial").unwrap();
    let recovered = build(&scan, &NeverCancel).unwrap();
    assert_eq!(recovered.parsed_files, cuts.len());
    assert_eq!(recovered.reused_files, 12 - cuts.len());
    assert!(!temporary.exists());
    assert_eq!(std::fs::read(artifact(&job_dir, "f02.ts")).unwrap(), whole);
    scan.refresh = true;
    equivalent(recovered, build(&scan, &NeverCancel).unwrap());
}

/// Deletes every artifact once this build's linking starts, so linking can
/// only read facts it kept in memory.
struct DeleteArtifactsAtLinking(PathBuf, std::sync::atomic::AtomicBool);
impl DeleteArtifactsAtLinking {
    fn new(job_dir: &Path) -> Self {
        Self(job_dir.to_path_buf(), false.into())
    }
}
impl CancellationCheck for DeleteArtifactsAtLinking {
    fn check(&self) -> Result<(), String> {
        let stage = Manifest::read(&self.0).map(|manifest| manifest.stage);
        let extracting = &self.1;
        if stage == Some(Stage::Extracting) {
            extracting.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        if stage == Some(Stage::Linking) && extracting.load(std::sync::atomic::Ordering::Relaxed) {
            for entry in std::fs::read_dir(&self.0).into_iter().flatten().flatten() {
                if entry.path().extension().is_some_and(|ext| ext == "fact") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        Ok(())
    }
}

/// Within the retention budget, extracted and reused facts link from
/// memory: each artifact is decoded at most once per build. Past it,
/// linking decodes the artifact from disk, to the same graph.
#[test]
fn facts_within_the_retention_budget_link_without_reading_artifacts_back() {
    let (_dir, mut scan) = fixture();
    let job_dir = job_path(scan.checkpoint.as_ref().unwrap(), &scan.root);
    let cold = build(&scan, &DeleteArtifactsAtLinking::new(&job_dir)).unwrap();
    assert_eq!(cold.parsed_files, 12);
    build(&scan, &NeverCancel).unwrap();
    let reused = build(&scan, &DeleteArtifactsAtLinking::new(&job_dir)).unwrap();
    assert_eq!(reused.reused_files, 12);
    equivalent(cold, reused);

    scan.limits.retained_bytes = 0;
    let spilled = build(&scan, &NeverCancel).unwrap();
    assert_eq!(
        spilled.reused_files, 0,
        "the deleted artifacts were rebuilt"
    );
    let reread = build(&scan, &NeverCancel).unwrap();
    assert_eq!(reread.reused_files, 12);
    assert!(
        build(&scan, &DeleteArtifactsAtLinking::new(&job_dir)).is_err(),
        "past the budget, linking reads its artifacts back"
    );
    scan.limits.retained_bytes = LIMITS.retained_bytes;
    equivalent(spilled, reread);
}
