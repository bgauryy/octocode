//! What a checkpoint writes: the extraction record a restart reads, only for
//! sources without a secret, and reused artifacts kept rather than written
//! again.
use super::*;

fn token() -> String {
    format!("ghp_{}", &"a1B2c3D4e5F6g7H8i9J0".repeat(2)[..36])
}

fn artifact(scan: &GraphScan, name: &str) -> PathBuf {
    job_path(scan.checkpoint.as_ref().unwrap(), &scan.root).join(format!(
        "{}.fact",
        octocode_engine::digest::sha256(name.as_bytes())
    ))
}

/// `count` exported functions, each calling the next: a source whose facts
/// run to megabytes.
fn dense_source(count: usize, tail: &str) -> String {
    let mut text = (0..count)
        .map(|id| {
            format!(
                "export function f{id}(a, b) {{ return f{}(b, a); }}\n",
                id + 1
            )
        })
        .collect::<String>();
    text.push_str(tail);
    text
}

/// The secret scan reads each file's source once: a secret anywhere in it,
/// in a comment the facts never hold or in an extracted literal, keeps that
/// file's facts off disk, while a clean file is checkpointed. A restart
/// resumes the clean files, parses the withheld ones again, and links the
/// same graph.
#[test]
fn a_source_holding_a_secret_is_never_checkpointed() {
    let (dir, scan) = fixture();
    let token = token();
    dir.write(
        "comment.ts",
        format!("// deploy key {token}\nexport function c() {{ return 1; }}\n"),
    );
    dir.write(
        "literal.ts",
        format!("import key from '{token}';\nexport const l = () => key;\n"),
    );
    let built = build(&scan, &NeverCancel).unwrap();
    assert_eq!(built.construction.memory_only_files, 2);
    assert_eq!(built.construction.checkpointed_files, 12);
    assert!(!artifact(&scan, "comment.ts").exists());
    assert!(!artifact(&scan, "literal.ts").exists());
    assert!(artifact(&scan, "f00.ts").exists());
    let job_dir = job_path(scan.checkpoint.as_ref().unwrap(), &scan.root);
    for entry in std::fs::read_dir(&job_dir).unwrap().map(Result::unwrap) {
        assert!(!String::from_utf8_lossy(&std::fs::read(entry.path()).unwrap()).contains(&token));
    }
    let retry = build(&scan, &NeverCancel).unwrap();
    assert_eq!(retry.construction.resumed_files, 12);
    assert_eq!(retry.construction.memory_only_files, 2);
    assert_eq!(retry.parsed_files, 2);
    equivalent(built, retry);
}

/// A large source is scanned whole: clean, it is checkpointed; with a secret
/// on its last line, never.
#[test]
fn a_large_source_is_scanned_whole() {
    let (dir, scan) = fixture();
    dir.write("big.ts", dense_source(7_000, ""));
    let clean = build(&scan, &NeverCancel).unwrap();
    assert_eq!(clean.construction.memory_only_files, 0);
    assert!(artifact(&scan, "big.ts").exists());
    dir.write("big.ts", dense_source(7_000, &format!("// {}\n", token())));
    let secret = build(&scan, &NeverCancel).unwrap();
    assert_eq!(secret.construction.memory_only_files, 1);
    assert!(!artifact(&scan, "big.ts").exists());
}

/// An artifact holds exactly what a restart reads: the extraction record
/// [`Job::load`] decodes, with no source correspondence a restart captures
/// again from the current source. Every checkpointed byte is one a resumed
/// build reads back.
#[test]
fn an_artifact_holds_only_what_a_restart_reads() {
    let (_dir, scan) = fixture();
    let job_dir = job_path(scan.checkpoint.as_ref().unwrap(), &scan.root);
    build(&scan, &AfterCommitted(job_dir.clone())).unwrap_err();
    let mut artifacts = 0;
    for entry in std::fs::read_dir(&job_dir).unwrap().map(Result::unwrap) {
        if entry.path().extension().is_none_or(|ext| ext != "fact") {
            continue;
        }
        let bytes = std::fs::read(entry.path()).unwrap();
        let record: cache::OwnedArtifact = serde_json::from_slice(&bytes[65..])
            .expect("the artifact is the extraction record, nothing more");
        assert!(!record.facts.declarations.is_empty());
        artifacts += 1;
    }
    assert!(artifacts >= 2);
}

/// A file reused from its own verified artifact keeps that artifact: after
/// one edit, only the edited file's artifact is written again, and the build
/// still equals a clean one.
#[test]
fn reused_artifacts_are_kept_not_rewritten() {
    let (dir, scan) = fixture();
    build(&scan, &NeverCancel).unwrap();
    let written = |name: &str| {
        std::fs::metadata(artifact(&scan, name))
            .unwrap()
            .modified()
            .unwrap()
    };
    let before = written("f01.ts");
    std::thread::sleep(std::time::Duration::from_millis(20));
    dir.write("f00.ts", "export function f0() { return 100; }\n");
    let edited = build(&scan, &NeverCancel).unwrap();
    assert_eq!(
        written("f01.ts"),
        before,
        "an unchanged file's artifact was rewritten"
    );
    assert_eq!(edited.construction.checkpointed_files, 12);
    let clean = build(
        &GraphScan {
            checkpoint: None,
            ..scan.clone()
        },
        &NeverCancel,
    )
    .unwrap();
    equivalent(edited, clean);
}
