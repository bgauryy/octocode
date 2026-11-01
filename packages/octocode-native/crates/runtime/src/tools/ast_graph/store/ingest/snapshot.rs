//! Stage, encode, name and publish a built snapshot, then apply retention.
//! The caller holds the build lock.
use super::*;

/// A built and projected scope, ready to publish.
pub(super) struct Publish<'a> {
    pub options: &'a IngestOptions,
    pub home: &'a Path,
    pub workspace: &'a Path,
    pub built: &'a super::super::super::types::BuiltGraph,
    pub projection: &'a tables::Projection,
    pub inputs: &'a BTreeMap<String, String>,
    pub build_ms: u64,
    pub registry: RegistryView<'a>,
    pub registry_identity: &'a str,
    pub limits: &'a Limits,
}

/// Names the narrower scopes to ingest on a budget the scan could not bound.
pub(super) fn too_broad(
    built: &super::super::super::types::BuiltGraph,
    mut output: GraphOutput,
) -> GraphOutput {
    let files = built.facts.keys().map(String::as_str);
    super::super::super::graph::scope::hint_narrower_scopes(&mut output.value, &built.root, files);
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
            return GraphOutput::error(
                ErrorCode::FileAccessFailed,
                format!("cannot stage graph: {error}"),
            );
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
            return too_broad(job.built, GraphOutput::error(error.code, error.message));
        }
    };
    let slug = scope_slug(&job.built.root, job.workspace);
    let (stamp, created_at) = utc_now();
    // Roots whose names map to one slug (`a-b`, `a/b`) still get distinct ids.
    let root_tag =
        &octocode_engine::digest::sha256(job.built.root.to_string_lossy().as_bytes())[..6];
    let id = match next_snapshot_id(job.home, &format!("{stamp}-{slug}-{root_tag}")) {
        Ok(id) => id,
        Err(error) => return GraphOutput::error(ErrorCode::FileAccessFailed, error),
    };
    let manifest = manifest(&ManifestInput {
        id: &id,
        scope: &slug,
        created_at: &created_at,
        workspace: job.workspace,
        built: job.built,
        projection: job.projection,
        bytes,
        digest: &digest,
        exclude_dir: &job.options.exclude_dir,
        language_globs: &job.options.language_globs,
        inputs: job.inputs,
        build_ms: job.build_ms,
        registry: job.registry,
        registry_identity: job.registry_identity,
    });
    if let Err(error) = cancel.check() {
        return GraphOutput::error(ErrorCode::Cancelled, error);
    }
    let dir = match staged.publish(job.home, &id, &manifest) {
        Ok(dir) => dir,
        Err(error) => {
            return GraphOutput::error(
                ErrorCode::FileAccessFailed,
                format!(
                    "cannot finish publishing graph {id} at {}: {error}",
                    job.home.join(&id).display()
                ),
            );
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
    progress::emit(|| format!("published {id} ({bytes} bytes)"));
    let mut receipt = json!({
        "id": id,
        "dir": dir.to_string_lossy(),
        "root": job.built.root.to_string_lossy(),
        "bytes": bytes,
        "sha256": digest,
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
    if !pruned.is_empty() {
        receipt["pruned"] = json!(pruned);
    }
    GraphOutput::ok(receipt)
}
