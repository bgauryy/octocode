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
    /// What changed after the build read it, if anything did.
    pub changed: Option<Changed>,
    pub scope_proof: Option<&'a Value>,
    pub build_ms: u64,
    pub registry: RegistryView<'a>,
    pub registry_identity: &'a str,
    pub build_identity: &'a str,
    pub limits: &'a Limits,
    /// What the SCIP tier did, and why it failed when it did.
    pub scip: (super::super::ScipStatus, Option<&'a str>),
}

/// Sources and configs that changed after the build read them.
pub(super) struct Changed {
    pub files: usize,
    /// The scope as graphQuery names it.
    pub path: String,
    /// The same graphBuild, which reads them again.
    pub rebuild: Option<Value>,
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
    octocode_runtime_base::response::paging::mark_partial(
        &mut out.value,
        octocode_runtime_base::response::paging::PartialReason::ScanBudget,
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
    let scope = job.built.file_scope.as_deref().unwrap_or(&job.built.root);
    let slug = scope_slug(scope, job.workspace);
    let (stamp, created_at) = utc_now();
    // Roots whose names map to one slug (`a-b`, `a/b`) still get distinct ids.
    let root_tag = root_tag(scope);
    let id = match next_snapshot_id(job.home, &format!("{stamp}-{slug}-{root_tag}")) {
        Ok(id) => id,
        Err(error) => return publication::write_failed(error),
    };
    let unreadable_dirs = job.built.unreadable_dirs.clone();
    let mut manifest = manifest(&ManifestInput {
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
        scip: job.scip,
    });
    if let Some(file) = &job.built.file_scope {
        // The persisted identity must also be exact for readers predating file scopes.
        manifest["root"] = json!(file);
        manifest["singleFile"] = json!(true);
    }
    if let Err(error) = cancel.check() {
        return GraphOutput::error(ErrorCode::Cancelled, error);
    }
    // Select the predecessor under the same publication lock as the new head.
    let baseline = unchanged::newest(job.home, &scope.to_string_lossy())
        .filter(|(_, previous)| {
            previous["buildIdentity"] == manifest["buildIdentity"]
                && previous["graph"]["sha256"] != manifest["graph"]["sha256"]
        })
        .map(|(previous, _)| previous);
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
    let pruned = prune(job.home, scope, job.options.keep);
    // Only this build's root: the home also holds other roots' jobs. Every
    // job asks when its root was published; one pass of the home answers all.
    let published = std::cell::OnceCell::new();
    let unfinished = super::super::super::graph::checkpoint::prune_jobs_of(
        &job.home.join(".facts"),
        &|root| {
            published
                .get_or_init(|| super::super::snapshots::published_roots(job.home))
                .get(root.to_string_lossy().as_ref())
                .copied()
        },
        Some(scope),
    );
    progress::emit(|| format!("published {id} ({bytes} bytes)"));
    let mut receipt = receipt(&id, &dir, &manifest, started);
    if let Some(previous) = baseline.filter(|previous| !pruned.iter().any(|p| p.id == *previous)) {
        let query = json!({"operation": "drift", "graph": id, "baseline": previous});
        let lead = octocode_runtime_base::response::result::Continuation::related(
            octocode_runtime_base::id::ToolId::GraphQuery,
            query,
        )
        .build();
        octocode_runtime_base::response::result::attach_related(&mut receipt, lead);
    }
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
        octocode_runtime_base::response::paging::push_warnings(&mut receipt, warnings);
    }
    octocode_runtime_base::response::paging::push_warnings(
        &mut receipt,
        uncheckpointed_warning(&job.built.construction),
    );
    if let Some(changed) = &job.changed {
        changed_meanwhile(&mut receipt, &id, changed);
    }
    answer(receipt)
}

/// A snapshot of files that changed after the build read them: the warning
/// counts them, and the related graphQuery stale lists each while the
/// related graphBuild reads them again (only they are parsed again).
fn changed_meanwhile(receipt: &mut Value, id: &str, changed: &Changed) {
    octocode_runtime_base::response::paging::push_warnings(
        receipt,
        [format!(
            "{} while the graph was built: the snapshot holds them as first read, so graphQuery stale lists them. Once the edits settle, the same graphBuild links their new content, parsing only the changed files again.",
            octocode_engine::text::counted(changed.files, "file changed", "files changed")
        )],
    );
    let stale = json!({"operation": "stale", "graph": id, "path": changed.path});
    let leads = std::iter::once(
        octocode_runtime_base::response::result::Continuation::related(
            octocode_runtime_base::id::ToolId::GraphQuery,
            stale,
        )
        .build(),
    )
    .chain(changed.rebuild.clone());
    for lead in leads {
        octocode_runtime_base::response::result::attach_related(receipt, lead);
    }
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
        "graph": id,
        "dir": dir.to_string_lossy(),
        "root": manifest.get("fileScope").unwrap_or(&manifest["root"]),
        "bytes": manifest["graph"]["bytes"],
        "sha256": manifest["graph"]["sha256"],
        "counts": manifest["counts"],
        "calls": manifest["calls"],
        "argumentCoverage": manifest["argumentCoverage"],
        "extraction": manifest["extraction"],
        "linkage": manifest["linkage"],
        "scan": super::super::manifest::scan_named(&manifest["scan"]),
        "buildMs": manifest["buildMs"],
        "totalMs": octocode_runtime_base::num::millis_of(started.elapsed()),
    });
    if manifest["diagnostics"]["total"].as_u64().unwrap_or(0) > 0 {
        receipt["diagnostics"] = manifest["diagnostics"]["byCode"].clone();
    }
    // A fresh build and a reused one state the tier the snapshot was built with.
    if let Some(status) = manifest["scip"].as_str() {
        receipt["scip"] = json!(status);
    }
    if manifest["scip"] == "failed" {
        let why = manifest["scipFailure"]
            .as_str()
            .unwrap_or("no reason was recorded");
        octocode_runtime_base::response::paging::push_warnings(
            &mut receipt,
            vec![format!(
                "The SCIP index failed ({why}); this snapshot holds syntax call edges only, and the next build runs the indexer again."
            )],
        );
    }
    if let Some((warning, build)) = super::super::query::package_above::widening(manifest) {
        octocode_runtime_base::response::paging::push_warnings(&mut receipt, [warning]);
        receipt["related"] = Value::Array(vec![build]);
    }
    let unreadable = manifest["unreadableDirs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    if !unreadable.is_empty() {
        octocode_runtime_base::response::paging::push_warnings(
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

#[cfg(test)]
mod pruned_tests;
