//! A published build's extraction artifacts stay as the scope's fact cache:
//! the next build reuses every file whose bytes did not change and links all
//! files again, so what it publishes equals a cold build's.
use super::*;

fn job_dir(scan: &GraphScan) -> PathBuf {
    job_path(scan.checkpoint.as_ref().unwrap(), &scan.root)
}

fn artifact(scan: &GraphScan, name: &str) -> PathBuf {
    job_dir(scan).join(format!(
        "{}.fact",
        octocode_engine::digest::sha256(name.as_bytes())
    ))
}

/// What publication does with the job of a build it published.
fn publish(scan: &GraphScan, built: &BuiltGraph) {
    settle(
        &job_dir(scan),
        built.construction.checkpoint_generation.as_deref().unwrap(),
    )
    .unwrap();
}

/// The same tree built with no cache at all.
fn cold(scan: &GraphScan) -> BuiltGraph {
    build(
        &GraphScan {
            checkpoint: None,
            ..scan.clone()
        },
        &NeverCancel,
    )
    .unwrap()
}

fn built(scan: &GraphScan) -> BuiltGraph {
    let built = build(scan, &NeverCancel).unwrap();
    publish(scan, &built);
    built
}

fn artifacts(scan: &GraphScan) -> usize {
    std::fs::read_dir(job_dir(scan))
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "fact"))
                .count()
        })
        .unwrap_or(0)
}

/// T1: after a publication, one edit extracts the edited file alone and
/// links the graph a cold build links.
#[test]
fn an_edit_reuses_every_file_but_the_edited_one() {
    let (dir, scan) = fixture();
    built(&scan);
    dir.write("f00.ts", "export function f0() { return 100; }\n");
    let edited = build(&scan, &NeverCancel).unwrap();
    assert_eq!(edited.parsed_files, 1);
    assert_eq!(edited.reused_files, 11);
    assert_eq!(
        edited.construction.resumed_files, 0,
        "a published build is a cache, not an interrupted job"
    );
    equivalent(edited, cold(&scan));
}

/// T3a: a file reused unchanged still links against the changed export of
/// the file it imports.
#[test]
fn a_renamed_export_updates_the_importer_that_was_reused() {
    let (dir, scan) = fixture();
    dir.write("a.ts", "export function foo() { return 1; }\n");
    dir.write(
        "b.ts",
        "import { foo } from './a';\nexport const b = () => foo();\n",
    );
    let first = built(&scan);
    dir.write("a.ts", "export function bar() { return 1; }\n");
    let edited = build(&scan, &NeverCancel).unwrap();
    assert_eq!(edited.parsed_files, 1);
    assert_eq!(edited.reused_files, 13);
    let cold = cold(&scan);
    assert_ne!(
        format!("{:?}", first.facts["a.ts"].declarations),
        format!("{:?}", cold.facts["a.ts"].declarations),
        "the rename changes a's exports"
    );
    equivalent(edited, cold);
}

/// T3b, T3c: a new file resolves an import that was unresolved, and a
/// deleted one unresolves it, with the importer's facts reused each time.
#[test]
fn new_and_deleted_files_relink_the_importers_that_were_reused() {
    let (dir, scan) = fixture();
    dir.write(
        "b.ts",
        "import { c } from './c';\nexport const b = () => c();\n",
    );
    built(&scan);
    dir.write("c.ts", "export function c() { return 1; }\n");
    let added = build(&scan, &NeverCancel).unwrap();
    assert_eq!(added.parsed_files, 1);
    assert_eq!(added.reused_files, 13);
    publish(&scan, &added);
    equivalent(added, cold(&scan));
    std::fs::remove_file(dir.0.join("c.ts")).unwrap();
    let removed = build(&scan, &NeverCancel).unwrap();
    assert_eq!(removed.parsed_files, 0);
    assert_eq!(removed.reused_files, 13);
    assert!(
        !artifact(&scan, "c.ts").exists(),
        "a deleted file's artifact goes at the next open"
    );
    equivalent(removed, cold(&scan));
}

/// T3d: a resolution config that appears after a publication relinks every
/// file from the cached facts.
#[test]
fn a_changed_resolution_config_relinks_from_the_cached_facts() {
    let (dir, scan) = fixture();
    built(&scan);
    dir.write("tsconfig.json", r#"{"compilerOptions":{"baseUrl":"."}}"#);
    let changed = build(&scan, &NeverCancel).unwrap();
    assert_eq!(changed.parsed_files, 0);
    assert_eq!(changed.reused_files, 12);
    equivalent(changed, cold(&scan));
}

/// T4: a policy that now hides a value of a file whose clean facts are
/// cached never serves the cached, unscrubbed facts, and never keeps them.
#[test]
fn a_stricter_policy_never_reuses_an_unscrubbed_artifact() {
    let (dir, scan) = fixture();
    let marker = "stricter_policy_codename";
    dir.write("f00.ts", format!("export const {marker} = 1;\n"));
    built(&scan);
    assert!(artifact(&scan, "f00.ts").exists());
    inject_stricter_judgment(marker);
    let stricter = build(&scan, &NeverCancel).unwrap();
    assert_eq!(stricter.construction.memory_only_files, 1);
    assert!(
        !artifact(&scan, "f00.ts").exists(),
        "the hidden file's old artifact is removed"
    );
    for entry in std::fs::read_dir(job_dir(&scan))
        .unwrap()
        .map(Result::unwrap)
    {
        assert!(!stored_text(&entry.path()).contains(marker));
    }
    assert!(
        !format!("{:?}", stricter.facts["f00.ts"].declarations).contains(marker),
        "linked facts are the scrubbed ones"
    );
    equivalent(stricter, cold(&scan));
}

/// T5: a flipped byte, a torn file, a stray temporary and a torn manifest of
/// a settled cache are misses: the files extract again and the graph equals
/// a cold build's.
#[test]
fn a_corrupt_settled_cache_is_a_miss_never_an_error() {
    let (_dir, scan) = fixture();
    built(&scan);
    let flipped = artifact(&scan, "f01.ts");
    let mut bytes = std::fs::read(&flipped).unwrap();
    let at = bytes.len() / 2;
    bytes[at] ^= 0xff;
    std::fs::write(&flipped, bytes).unwrap();
    let torn = artifact(&scan, "f02.ts");
    let bytes = std::fs::read(&torn).unwrap();
    std::fs::write(&torn, &bytes[..bytes.len() / 2]).unwrap();
    std::fs::write(job_dir(&scan).join(".f03.tmp"), b"partial").unwrap();
    let recovered = build(&scan, &NeverCancel).unwrap();
    assert_eq!(recovered.parsed_files, 2);
    assert_eq!(recovered.reused_files, 10);
    publish(&scan, &recovered);
    equivalent(recovered, cold(&scan));
    std::fs::write(job_dir(&scan).join("job.json"), b"{\"torn").unwrap();
    let rebuilt = build(&scan, &NeverCancel).unwrap();
    assert_eq!(rebuilt.reused_files, 0, "no manifest, no cache");
    equivalent(rebuilt, cold(&scan));
}

/// T6: only what changes a file's facts drops the cache; what changes which
/// files are scanned does not.
#[test]
fn only_a_fact_affecting_change_drops_the_cache() {
    let (_dir, scan) = fixture();
    built(&scan);
    let mut narrower = scan.clone();
    narrower.exclude = vec!["f11.ts".into()];
    let excluded = build(&narrower, &NeverCancel).unwrap();
    assert_eq!(excluded.parsed_files, 0, "an exclude is not a fact input");
    assert_eq!(excluded.reused_files, 11);
    publish(&narrower, &excluded);
    let mut parser = scan.clone();
    parser
        .language_globs
        .insert("javascript".into(), vec!["**/*.ts".into()]);
    let reparsed = build(&parser, &NeverCancel).unwrap();
    assert_eq!(reparsed.reused_files, 0);
    assert_eq!(reparsed.parsed_files, 12);
    publish(&parser, &reparsed);
    // Another schema, package or registry is another namespace: the job
    // record names it, and an artifact of another one never loads.
    let record = job_dir(&scan).join("job.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();
    manifest["namespace"] = serde_json::json!("another facts schema");
    std::fs::write(&record, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let other = build(&parser, &NeverCancel).unwrap();
    assert_eq!(other.reused_files, 0);
    assert_eq!(other.parsed_files, 12);
}

/// T6: a cache of an older job version is refused by the owner of the newer
/// one, and an extraction fact of another analysis version never loads.
#[test]
fn a_cache_of_an_older_job_version_is_not_reused() {
    let (_dir, scan) = fixture();
    built(&scan);
    let record = job_dir(&scan).join("job.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();
    manifest["version"] = serde_json::json!(JOB_VERSION - 1);
    std::fs::write(&record, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let next = build(&scan, &NeverCancel).unwrap();
    assert_eq!(next.reused_files, 0);
    assert_eq!(next.parsed_files, 12);
}

/// T7: two builds of one scope serialize on its job lock, and the second
/// reuses what the first wrote.
#[test]
fn two_builds_of_one_scope_share_the_cache() {
    let (_dir, scan) = fixture();
    let results = std::thread::scope(|threads| {
        let workers = [0, 1].map(|_| threads.spawn(|| build(&scan, &NeverCancel).unwrap()));
        workers.map(|worker| worker.join().unwrap())
    });
    let parsed = results
        .iter()
        .map(|built| built.parsed_files)
        .sum::<usize>();
    let reused = results
        .iter()
        .map(|built| built.reused_files)
        .sum::<usize>();
    assert_eq!((parsed, reused), (12, 12));
    let [first, second] = results;
    equivalent(first, second);
}

/// T8: the cache is bounded by the disk budget: a lowered budget evicts the
/// largest artifacts at open instead of refusing the build.
#[test]
fn a_lowered_disk_budget_evicts_instead_of_refusing() {
    let (_dir, mut scan) = fixture();
    built(&scan);
    let size = std::fs::metadata(artifact(&scan, "f00.ts")).unwrap().len();
    scan.limits.spill_bytes = size * 4;
    let next = build(&scan, &NeverCancel).unwrap();
    assert!(next.reused_files > 0 && next.parsed_files > 0);
    assert_eq!(next.reused_files + next.parsed_files, 12);
    assert!(artifacts(&scan) <= 4);
    equivalent(next, cold(&scan));
}

/// A cache artifact that cannot be written costs only that file's
/// resumption: the build links its decoded facts.
#[test]
fn an_artifact_that_cannot_be_written_is_uncheckpointed_not_fatal() {
    let (_dir, scan) = fixture();
    let policy = workspace_policy(&scan.root);
    let job = Job::open(
        &scan,
        &scan.root,
        &[],
        &policy,
        RegistryView::installed(),
        &NeverCancel,
    )
    .unwrap()
    .unwrap();
    let text = "export const main = () => 1;\n";
    let facts = octocode_engine::signatures::extract_graph_facts(text, "main.ts").unwrap();
    let digest = octocode_engine::digest::sha256(text.as_bytes());
    let stop = || false;
    let mut budget = octocode_engine::text::analysis::MapBudget::new(
        octocode_engine::text::analysis::MapLimits::default(),
        crate::execution::cancel::deadline_of(&NeverCancel),
        &stop,
    );
    let admitted = octocode_engine::text::analysis::AdmittedSource::from_bytes(
        text.as_bytes(),
        "main.ts",
        &mut budget,
    )
    .unwrap();
    let bound = facts.source_templates.bind(&admitted, &mut budget).unwrap();
    let view = GraphFactsView {
        relative_path: "main.ts",
        content: text,
        content_digest: &digest,
        source_stamp: None,
        facts: &facts,
        admitted: &admitted,
        source: &bound,
        reference_counts: &[],
        reused: false,
        memory: None,
    };
    let source = capture::capture(&view, &NeverCancel, LIMITS.source_map()).unwrap();
    std::fs::create_dir(job.target("main.ts")).unwrap();
    assert!(matches!(
        job.keep(&view, source, false),
        Ok((super::super::scan::Kept::Memory(_), _))
    ));
    assert_eq!(job.progress().uncheckpointed_files, 1);
    assert_eq!(job.progress().checkpointed_files, 0);
}

/// T9: a memory-only build touches no cache.
#[test]
fn a_build_without_a_checkpoint_writes_no_cache() {
    let (_dir, scan) = fixture();
    cold(&scan);
    assert!(!job_dir(&scan).exists());
}
