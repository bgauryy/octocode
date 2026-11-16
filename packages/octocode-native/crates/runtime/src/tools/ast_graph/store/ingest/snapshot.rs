//! Stage, encode, name and publish a built snapshot, then apply retention.
//! The caller holds the build lock.
use super::*;

/// A built and projected scope, ready to publish.
pub(super) struct Publish<'a> {
    pub options: &'a IngestOptions,
    pub home: &'a Path,
    pub workspace: &'a Path,
    /// The build; its linked facts are already freed ([`Self::files`]).
    pub built: &'a super::super::super::types::BuiltGraph,
    /// The scanned files, root-relative.
    pub files: &'a [String],
    pub projection: &'a tables::Projection,
    pub argument_coverage: &'a Value,
    pub inputs: &'a BTreeMap<String, String>,
    pub input_proof: Option<&'a str>,
    pub scope_proof: Option<&'a Value>,
    pub build_ms: u64,
    pub registry: RegistryView<'a>,
    pub registry_identity: &'a str,
    pub build_identity: &'a str,
    pub limits: &'a Limits,
}

/// A build whose extraction paused at its call's budget: a partial answer
/// with its progress; the tool adapter adds the call that resumes it.
pub(super) fn paused(
    root: &Path,
    pause: super::super::super::types::ExtractionPause,
) -> GraphOutput {
    let mut out = GraphOutput::ok(json!({
        "root": root.to_string_lossy(),
        "progress": {
            "committedFiles": pause.committed_files,
            "remainingFiles": pause.remaining_files,
        },
    }));
    crate::response::paging::mark_partial(
        &mut out.value,
        crate::response::paging::PartialReason::ScanBudget,
        &[],
    );
    out
}

/// Names the narrower scopes to ingest on a budget the scan could not bound.
pub(super) fn too_broad<'a>(
    root: &Path,
    scope: &super::super::super::build_scope::BuildScope,
    files: impl Iterator<Item = &'a str>,
    mut output: GraphOutput,
) -> GraphOutput {
    super::super::super::graph::scope::hint_narrower_scopes(&mut output.value, root, scope, files);
    output
}

/// Publishes `job` as a new snapshot and returns its receipt.
pub(super) fn publish_snapshot(
    job: &Publish<'_>,
    cancel: &dyn CancellationCheck,
    started: Instant,
) -> GraphOutput {
    let staged = match publication::Staged::new(job.home) {
        Ok(staged) => staged,
        Err(error) => {
            return publication::write_failed(format!("cannot stage graph: {error}"));
        }
    };
    progress::emit(|| "encoding snapshot".to_owned());
    let (bytes, digest) = match format::write_for_publish(
        &job.projection.tables,
        &staged.graph(),
        job.limits,
        cancel,
    ) {
        Ok(encoded) => encoded,
        Err(error) => {
            return too_broad(
                &job.built.root,
                &super::super::super::build_scope::BuildScope::of_options(job.options),
                job.files.iter().map(String::as_str),
                GraphOutput::error(error.code, error.message),
            );
        }
    };
    let slug = scope_slug(&job.built.root, job.workspace);
    let (stamp, created_at) = utc_now();
    // Roots whose names map to one slug (`a-b`, `a/b`) still get distinct ids.
    let root_tag = root_tag(&job.built.root);
    let id = match next_snapshot_id(job.home, &format!("{stamp}-{slug}-{root_tag}")) {
        Ok(id) => id,
        Err(error) => return publication::write_failed(error),
    };
    let unreadable_dirs = if job.built.files_skipped > 0 {
        unreadable_directories(&job.built.root, job.files)
    } else {
        Vec::new()
    };
    let manifest = manifest(&ManifestInput {
        id: &id,
        scope: &slug,
        created_at: &created_at,
        workspace: job.workspace,
        built: job.built,
        files_scanned: job.files.len(),
        projection: job.projection,
        argument_coverage: job.argument_coverage,
        bytes,
        digest: &digest,
        exclude: &job.options.exclude,
        walk: job.options.walk,
        language_globs: &job.options.language_globs,
        inputs: job.inputs,
        input_proof: job.input_proof,
        scope_proof: job.scope_proof,
        build_ms: job.build_ms,
        registry: job.registry,
        registry_identity: job.registry_identity,
        build_identity: job.build_identity,
        unreadable_dirs: &unreadable_dirs,
    });
    if let Err(error) = cancel.check() {
        return GraphOutput::error(ErrorCode::Cancelled, error);
    }
    let dir = match staged.publish(job.home, &id, &manifest) {
        Ok(dir) => dir,
        Err(error) => {
            return publication::write_failed(format!(
                "cannot finish publishing graph {id} at {}: {error}",
                job.home.join(&id).display()
            ));
        }
    };
    if let Err(error) = finish_checkpoint(
        job.options,
        job.home,
        job.built,
        &json!({"id": id, "dir": dir}),
    ) {
        return error;
    }
    let pruned = prune(job.home, &job.built.root, job.options.keep);
    // Only this build's root: the home also holds other roots' jobs.
    let unfinished = super::super::super::graph::checkpoint::prune_jobs_of(
        &job.home.join(".facts"),
        &|root| super::super::snapshots::last_published(job.home, root),
        Some(&job.built.root),
    );
    progress::emit(|| format!("published {id} ({bytes} bytes)"));
    let mut receipt = receipt(&id, &dir, &manifest, started);
    if unfinished > 0 {
        receipt["unfinishedJobBytes"] = json!(unfinished);
    }
    if !pruned.is_empty() {
        let warnings = pruned_warnings(&pruned, job.options.keep);
        receipt["pruned"] = json!(
            pruned
                .into_iter()
                .map(|pruned| pruned.id)
                .collect::<Vec<_>>()
        );
        crate::response::paging::push_warnings(&mut receipt, warnings);
    }
    crate::response::paging::push_warnings(
        &mut receipt,
        uncheckpointed_warning(&job.built.construction),
    );
    answer(receipt)
}

/// The files this build linked without a checkpoint, past the job's disk or
/// memory-only budget or one artifact's bound: the graph is whole, but
/// `extraction` alone would not say that an interrupted build parses them
/// again.
fn uncheckpointed_warning(progress: &super::super::super::types::BuildProgress) -> Option<String> {
    let files = progress.uncheckpointed_files;
    (files > 0).then(|| {
        format!(
            "{files} file(s) were linked without a checkpoint, past the job's budget (maxSpillBytes {} on disk, maxMemoryOnlyBytes {} for secret-bearing files) or one artifact's bound: the graph is complete, but an interrupted build parses them again.",
            progress.max_spill_bytes, progress.max_memory_only_bytes
        )
    })
}

/// The answer of a published snapshot's `receipt`: empty when its graph
/// holds no node, so nothing in the scope was graphable (its `scan` states
/// what the build read, its hints what it left out).
pub(super) fn answer(receipt: Value) -> GraphOutput {
    if receipt["counts"]["nodes"].as_u64() == Some(0) {
        GraphOutput::empty(receipt)
    } else {
        GraphOutput::ok(receipt)
    }
}

/// Why a build's prune removed what it removed, one sentence per reason
/// with its count: the snapshots of this root beyond `keep` (another
/// session's among them, if it built this root) and those in a format this
/// build cannot read. The IDs are the receipt's `pruned` list, stated
/// there alone; a pruned ID fails `notFound` from then on.
fn pruned_warnings(pruned: &[super::super::snapshots::Pruned], keep: usize) -> Vec<String> {
    let counted = |outdated: bool| {
        pruned
            .iter()
            .filter(|pruned| pruned.outdated == outdated)
            .count()
    };
    let mut warnings = Vec::new();
    let older = counted(false);
    if older > 0 {
        warnings.push(format!(
            "Pruned {older} older snapshot(s) of this root beyond keep:{keep}, whichever session built them; their IDs (pruned) now fail notFound. Raise keep to retain more."
        ));
    }
    let outdated = counted(true);
    if outdated > 0 {
        warnings.push(format!(
            "Pruned {outdated} snapshot(s) in an older graph format this build cannot read."
        ));
    }
    warnings
}

/// The receipt of the published snapshot `id` at `dir`, from its manifest:
/// a fresh build and an unchanged scope's reuse answer alike.
pub(super) fn receipt(id: &str, dir: &Path, manifest: &Value, started: Instant) -> Value {
    let mut receipt = json!({
        "id": id,
        "dir": dir.to_string_lossy(),
        "root": manifest["root"],
        "bytes": manifest["graph"]["bytes"],
        "sha256": manifest["graph"]["sha256"],
        "counts": manifest["counts"],
        "calls": manifest["calls"],
        "argumentCoverage": manifest["argumentCoverage"],
        "extraction": manifest["extraction"],
        "linkage": manifest["linkage"],
        "scan": manifest["scan"],
        "buildMs": manifest["buildMs"],
        "totalMs": crate::num::millis_of(started.elapsed()),
    });
    if manifest["diagnostics"]["total"].as_u64().unwrap_or(0) > 0 {
        receipt["diagnostics"] = manifest["diagnostics"]["byCode"].clone();
    }
    let unreadable = manifest["unreadableDirs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    if !unreadable.is_empty() {
        crate::response::paging::push_warnings(
            &mut receipt,
            [format!(
                "The walk could not read {} director{}, so the graph lacks everything below {}: {}.",
                unreadable.len(),
                if unreadable.len() == 1 { "y" } else { "ies" },
                if unreadable.len() == 1 { "it" } else { "them" },
                unreadable.join(", ")
            )],
        );
    }
    receipt
}

/// The directories beside the scanned files that the process may not read:
/// the walk counts them skipped (`scan.filesSkipped`) but names none. Each
/// directory a scanned file sits in is listed once, and a subdirectory no
/// scanned file sits in that cannot be opened is named, root-relative.
fn unreadable_directories(root: &Path, scanned: &[String]) -> Vec<String> {
    let mut known = std::collections::BTreeSet::<PathBuf>::new();
    known.insert(PathBuf::new());
    for file in scanned {
        let mut directory = Path::new(file).parent();
        while let Some(parent) = directory {
            if !known.insert(parent.to_path_buf()) {
                break;
            }
            directory = parent.parent();
        }
    }
    let mut unreadable = std::collections::BTreeSet::new();
    for relative in &known {
        let Ok(entries) = std::fs::read_dir(root.join(relative)) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = relative.join(entry.file_name());
            if known.contains(&path) || !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            if std::fs::read_dir(entry.path())
                .is_err_and(|error| error.kind() == std::io::ErrorKind::PermissionDenied)
            {
                unreadable.insert(path.to_string_lossy().into_owned());
            }
        }
    }
    unreadable.into_iter().collect()
}

#[cfg(test)]
mod pruned_tests {
    use super::super::super::snapshots::Pruned;
    use super::pruned_warnings;

    /// Files linked past the checkpoint budgets get one warning naming
    /// their count and the budgets; a build that checkpointed every file
    /// warns of nothing.
    #[test]
    fn files_past_the_checkpoint_budget_are_disclosed() {
        let mut progress = super::super::super::super::types::BuildProgress {
            max_spill_bytes: 2 << 30,
            max_memory_only_bytes: 64 << 20,
            ..Default::default()
        };
        assert_eq!(super::uncheckpointed_warning(&progress), None);
        progress.uncheckpointed_files = 7_176;
        let warning = super::uncheckpointed_warning(&progress).expect("warning");
        assert!(warning.contains("7176 file(s)"), "{warning}");
        assert!(warning.contains(&(2u64 << 30).to_string()), "{warning}");
    }

    /// A prune states each reason with its count; the removed IDs are the
    /// receipt's `pruned` list, so no warning repeats one.
    #[test]
    fn a_prune_states_each_reason_once_and_names_no_id_twice() {
        let pruned = [
            Pruned {
                id: "20261109T163855Z-a".into(),
                outdated: false,
            },
            Pruned {
                id: "20261109T181915Z-a".into(),
                outdated: false,
            },
            Pruned {
                id: "20250101T000000Z-b".into(),
                outdated: true,
            },
        ];
        let warnings = pruned_warnings(&pruned, 1);
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(
            warnings[0].contains("keep:1") && warnings[0].contains('2'),
            "{warnings:?}"
        );
        assert!(
            warnings[1].contains("older graph format") && warnings[1].contains('1'),
            "{warnings:?}"
        );
        for snapshot in &pruned {
            assert!(
                warnings
                    .iter()
                    .all(|warning| !warning.contains(&snapshot.id)),
                "{} repeated in {warnings:?}",
                snapshot.id
            );
        }
    }
}
