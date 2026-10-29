use super::*;
use crate::execution::cancel::NeverCancel;
use crate::tools::test_support::{Fixture, workspace_policy};

mod scan_and_reuse;

fn fixture() -> (Fixture, GraphScan) {
    let dir = Fixture::new();
    for id in 0..12 {
        dir.write(
            &format!("f{id:02}.ts"),
            format!("export function f{id}() {{ return {id}; }}\n"),
        );
    }
    let root = dir.0.canonicalize().unwrap();
    let scan = GraphScan {
        root: root.clone(),
        checkpoint: Some(root.join(".facts")),
        max_files: 50_000,
        ..Default::default()
    };
    (dir, scan)
}

fn build(scan: &GraphScan, cancel: &dyn CancellationCheck) -> Result<BuiltGraph, ToolError> {
    super::super::build_graph(
        scan,
        &workspace_policy(&scan.root),
        &ContentSecurity::new(),
        cancel,
    )
}

fn equivalent(mut left: BuiltGraph, mut right: BuiltGraph) {
    for built in [&mut left, &mut right] {
        built.construction = BuildProgress::default();
        built.parsed_files = 0;
        built.reused_files = 0;
        for facts in built.facts.values_mut() {
            facts.stamp = None;
        }
    }
    assert_eq!(format!("{left:?}"), format!("{right:?}"));
}

struct AfterCommitted(PathBuf);
impl CancellationCheck for AfterCommitted {
    fn check(&self) -> Result<(), String> {
        let count = std::fs::read_dir(&self.0)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "fact")
            })
            .count();
        if count >= 2 {
            Err("interrupted after committed extraction".into())
        } else {
            Ok(())
        }
    }
}

#[test]
fn interrupted_extraction_resumes_verified_facts_and_equals_clean_build() {
    let (_dir, mut scan) = fixture();
    let job_dir = job_path(scan.checkpoint.as_ref().unwrap(), &scan.root);
    assert_eq!(
        build(&scan, &AfterCommitted(job_dir.clone()))
            .unwrap_err()
            .code,
        ErrorCode::Cancelled
    );
    assert!(Manifest::read(&job_dir).is_some_and(|manifest| manifest.stage == Stage::Extracting));
    let resumed = build(&scan, &NeverCancel).unwrap();
    assert!(resumed.construction.resumed_files >= 2);
    assert_eq!(resumed.construction.checkpointed_files, 12);
    assert!(resumed.construction.resumed_files <= resumed.reused_files);
    scan.force = true;
    let clean = build(&scan, &NeverCancel).unwrap();
    assert_eq!(clean.construction.resumed_files, 0);
    equivalent(resumed, clean);
}

#[test]
fn changed_deleted_and_corrupt_artifacts_cannot_supply_stale_facts() {
    let (dir, mut scan) = fixture();
    build(&scan, &NeverCancel).unwrap();
    let job_dir = job_path(scan.checkpoint.as_ref().unwrap(), &scan.root);
    let target = job_dir.join(format!(
        "{}.fact",
        octocode_engine::digest::sha256(b"f03.ts")
    ));
    std::fs::write(target, b"corrupt committed artifact").unwrap();
    dir.write("f00.ts", "export function replaced() { return 99; }\n");
    std::fs::remove_file(dir.0.join("f01.ts")).unwrap();
    let recovered = build(&scan, &NeverCancel).unwrap();
    assert_eq!(recovered.construction.checkpointed_files, 11);
    assert_eq!(
        recovered.construction.resumed_files, 0,
        "inventory changed: extraction reused, job was not resumed"
    );
    assert!(
        !job_dir
            .join(format!(
                "{}.fact",
                octocode_engine::digest::sha256(b"f01.ts")
            ))
            .exists()
    );
    scan.force = true;
    equivalent(recovered, build(&scan, &NeverCancel).unwrap());
}

#[test]
fn completed_job_and_configuration_changes_are_reuse_not_resume() {
    let (dir, scan) = fixture();
    build(&scan, &NeverCancel).unwrap();
    let job_dir = job_path(scan.checkpoint.as_ref().unwrap(), &scan.root);
    retire(&job_dir).unwrap();
    assert!(!job_dir.exists(), "a retired job holds no disk");
    let reused = build(&scan, &NeverCancel).unwrap();
    assert_eq!(reused.construction.resumed_files, 0);
    dir.write("tsconfig.json", r#"{"compilerOptions":{"baseUrl":"."}}"#);
    let changed = build(&scan, &NeverCancel).unwrap();
    assert_eq!(changed.construction.resumed_files, 0);
    assert_eq!(changed.reused_files, 12);
    let mut parser = scan.clone();
    parser
        .language_globs
        .insert("javascript".into(), vec!["**/*.ts".into()]);
    assert_eq!(
        build(&parser, &NeverCancel)
            .unwrap()
            .construction
            .resumed_files,
        0
    );
}

#[test]
fn safe_to_secret_removes_obsolete_checkpoint_and_never_persists_secret() {
    let (dir, scan) = fixture();
    build(&scan, &NeverCancel).unwrap();
    let token = format!("ghp_{}", &"a1B2c3D4e5F6g7H8i9J0".repeat(2)[..36]);
    dir.write(
        "f00.ts",
        format!("import token from '{token}'; export const main = () => token;\n"),
    );
    let changed = build(&scan, &NeverCancel).unwrap();
    assert_eq!(changed.construction.memory_only_files, 1);
    assert_eq!(changed.construction.checkpointed_files, 11);
    let job_dir = job_path(scan.checkpoint.as_ref().unwrap(), &scan.root);
    assert!(
        !job_dir
            .join(format!(
                "{}.fact",
                octocode_engine::digest::sha256(b"f00.ts")
            ))
            .exists()
    );
    for entry in std::fs::read_dir(&job_dir).unwrap().map(Result::unwrap) {
        assert!(!String::from_utf8_lossy(&std::fs::read(entry.path()).unwrap()).contains(&token));
    }
    let retry = build(&scan, &NeverCancel).unwrap();
    assert_eq!(retry.construction.memory_only_files, 1);
    assert_eq!(retry.construction.resumed_files, 11);
    equivalent(changed, retry);
}

#[test]
fn checkpoint_open_refuses_corruption_that_would_exceed_disk_quota() {
    let (_dir, scan) = fixture();
    build(&scan, &NeverCancel).unwrap();
    let job_dir = job_path(scan.checkpoint.as_ref().unwrap(), &scan.root);
    let target = job_dir.join(format!(
        "{}.fact",
        octocode_engine::digest::sha256(b"f00.ts")
    ));
    std::fs::OpenOptions::new()
        .write(true)
        .open(target)
        .unwrap()
        .set_len(LIMITS.spill_bytes + 1)
        .unwrap();
    assert_eq!(
        build(&scan, &NeverCancel).unwrap_err().code,
        ErrorCode::ScopeTooBroad
    );
}

#[test]
fn checkpoint_write_errors_are_terminal_and_keep_prior_commits() {
    let (_dir, scan) = fixture();
    let job_dir = job_path(scan.checkpoint.as_ref().unwrap(), &scan.root);
    build(&scan, &AfterCommitted(job_dir.clone())).unwrap_err();
    let committed = std::fs::read_dir(&job_dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "fact")
        })
        .count();
    std::fs::remove_file(job_dir.join("job.json")).unwrap();
    std::fs::create_dir(job_dir.join("job.json")).unwrap();
    assert_eq!(
        build(&scan, &NeverCancel).unwrap_err().code,
        ErrorCode::FileAccessFailed
    );
    assert!(committed >= 2);
}

#[test]
fn provider_only_grammar_resumes_through_the_same_checkpoint_owner() {
    let (dir, mut scan) = fixture();
    dir.write("settings.toml", "answer = 42\n");
    let registry = octocode_engine::signatures::languages::conformance::toml_registry();
    let build = |scan: &GraphScan| {
        super::super::build_graph_with_registry(
            scan,
            &workspace_policy(&scan.root),
            &ContentSecurity::new(),
            &NeverCancel,
            registry,
        )
        .unwrap()
    };
    let first = build(&scan);
    assert_eq!(first.construction.checkpointed_files, 13);
    let resumed = build(&scan);
    assert_eq!(resumed.construction.resumed_files, 13);
    assert_eq!(
        resumed.facts["settings.toml"].declarations[0].name,
        "answer"
    );
    scan.force = true;
    equivalent(resumed, build(&scan));
}

#[test]
fn checksummed_but_invalid_facts_never_count_as_resumed() {
    for (field, value) in [
        ("schemaVersion", serde_json::json!(0)),
        ("sourceDigest", serde_json::json!("wrong source")),
        ("file", serde_json::json!("other.ts")),
    ] {
        let (dir, scan) = fixture();
        dir.write("settings.toml", "answer = 42\n");
        let registry = octocode_engine::signatures::languages::conformance::toml_registry();
        let build = || {
            super::super::build_graph_with_registry(
                &scan,
                &workspace_policy(&scan.root),
                &ContentSecurity::new(),
                &NeverCancel,
                registry,
            )
            .unwrap()
        };
        let expected = build();
        let job_dir = job_path(scan.checkpoint.as_ref().unwrap(), &scan.root);
        for entry in std::fs::read_dir(&job_dir).unwrap().map(Result::unwrap) {
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "fact") {
                continue;
            }
            let bytes = std::fs::read(&path).unwrap();
            let mut artifact: serde_json::Value = serde_json::from_slice(&bytes[65..]).unwrap();
            artifact["extraction"]["facts"][field] = value.clone();
            let body = serde_json::to_vec(&artifact).unwrap();
            let mut rewritten = octocode_engine::digest::sha256(&body).into_bytes();
            rewritten.push(b'\n');
            rewritten.extend(body);
            std::fs::write(path, rewritten).unwrap();
        }
        let recovered = build();
        assert_eq!(recovered.construction.resumed_files, 0, "{field}");
        assert_eq!(recovered.construction.checkpointed_files, 13, "{field}");
        equivalent(recovered, expected);
    }
}

#[test]
fn quotas_and_io_failures_do_not_commit_incomplete_artifacts() {
    let (dir, scan) = fixture();
    let policy = workspace_policy(&scan.root);
    let security = ContentSecurity::new();
    let job = Job::open(
        &scan,
        &scan.root,
        &[],
        &policy,
        &security,
        RegistryView::installed(),
    )
    .unwrap()
    .unwrap();
    let token = format!("ghp_{}", &"a1B2c3D4e5F6g7H8i9J0".repeat(2)[..36]);
    let text = format!("import value from '{token}'; export const main = value;\n");
    let facts: octocode_engine::graph::GraphFactsDocument = serde_json::from_str(
        &octocode_engine::signatures::extract_graph_facts(&text, "secret.ts").unwrap(),
    )
    .unwrap();
    let digest = octocode_engine::digest::sha256(text.as_bytes());
    let stop = || false;
    let mut budget = octocode_engine::text::analysis::MapBudget::new(
        octocode_engine::text::analysis::MapLimits::default(),
        std::time::Instant::now() + LIMITS.file_map_deadline,
        &stop,
    );
    let admitted = octocode_engine::text::analysis::AdmittedSource::from_bytes(
        text.as_bytes(),
        "secret.ts",
        &mut budget,
    )
    .unwrap();
    let bound = facts.source_templates.bind(&admitted, &mut budget).unwrap();
    let view = GraphFactsView {
        relative_path: "secret.ts",
        content: &text,
        content_digest: &digest,
        source_stamp: None,
        facts: &facts,
        admitted: &admitted,
        source: &bound,
        reference_counts: &[],
        reused: false,
    };
    let source = capture::capture(&view, &NeverCancel, &LIMITS).unwrap();
    job.state().memory_bytes = LIMITS.memory_only_bytes;
    assert_eq!(
        job.keep(&view, &source).err().unwrap().code,
        ErrorCode::ScopeTooBroad
    );
    assert_eq!(job.progress().checkpointed_files, 0);
    assert!(!job.target("secret.ts").exists());
    // The same lower-level durable writer reports real filesystem failure, not a cache miss.
    let blocked = dir.0.join("blocked");
    std::fs::create_dir(&blocked).unwrap();
    assert_eq!(
        state::durable_write(&blocked, b"incomplete")
            .unwrap_err()
            .code,
        ErrorCode::FileAccessFailed
    );
    assert!(blocked.is_dir());
}

#[test]
fn opening_a_job_removes_completed_jobs_and_keeps_unfinished_ones() {
    let (_dir, scan) = fixture();
    build(&scan, &NeverCancel).unwrap();
    let job_dir = job_path(scan.checkpoint.as_ref().unwrap(), &scan.root);
    let jobs = job_dir.parent().unwrap().to_path_buf();
    // Two jobs of other roots, copied from this one: one an earlier build
    // marked complete, one still unfinished.
    let (completed, unfinished) = (jobs.join("a".repeat(64)), jobs.join("b".repeat(64)));
    for target in [&completed, &unfinished] {
        std::fs::create_dir_all(target).unwrap();
        std::fs::copy(job_dir.join("job.json"), target.join("job.json")).unwrap();
    }
    let manifest = state::Manifest::read(&completed).unwrap();
    manifest.write(&completed, state::Stage::Complete).unwrap();
    build(&scan, &NeverCancel).unwrap();
    assert!(!completed.exists(), "a completed job is removed");
    assert!(unfinished.exists(), "an unfinished job stays resumable");
}
