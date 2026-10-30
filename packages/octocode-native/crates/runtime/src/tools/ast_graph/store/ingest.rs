//! `graph ingest`: build, redact and publish a snapshot (or reuse the
//! current one).
use super::*;
use crate::tools::ast_graph::limits::{LIMITS, Limits};
use octocode_engine::signatures::languages::RegistryView;
mod publication;
mod redact;
#[cfg(test)]
pub(super) use redact::PASSES as REDACTIONS;
pub(super) use redact::redact_secrets;

/// Builds the graph for `options.path` and publishes a new snapshot.
/// `cargo` is the configured `OCTOCODE_CARGO` for Rust workspace linking.
pub fn ingest(
    options: &IngestOptions,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
    cargo: Option<&str>,
) -> GraphOutput {
    ingest_with_registry(
        options,
        paths,
        security,
        cancel,
        cargo,
        RegistryView::installed(),
        &LIMITS,
    )
}

/// [`ingest`] with the parser registry and the snapshot bounds it builds
/// under (tests lower them).
pub(super) fn ingest_with_registry(
    options: &IngestOptions,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
    cargo: Option<&str>,
    registry: RegistryView<'_>,
    limits: &Limits,
) -> GraphOutput {
    let started = Instant::now();
    let home = match graph_home(options.workspace.as_deref()) {
        Ok(home) => home,
        Err(message) => return GraphOutput::error(ErrorCode::WorkspaceRootInvalid, message),
    };
    let home = match paths.validate_output(&home) {
        Ok(valid) => valid.canonical,
        Err(error) => {
            return GraphOutput::error(error.shared_code(), error.message);
        }
    };
    let _lock = match super::build_lock::acquire(&home, cancel, paths) {
        Ok(lock) => lock,
        Err(error) => return error,
    };
    if let Err(error) = publication::cleanup(&home, paths, cancel) {
        return error;
    }
    let workspace = home
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let max_files = options.max_files.unwrap_or(limits.max_files);
    let registry_identity = match super::super::graph::registry_identity(registry) {
        Ok(identity) => identity,
        Err(error) => return GraphOutput::error(error.code, error.message),
    };
    if !options.force
        && let Ok(root) = paths.validate(&options.path).map(|valid| valid.canonical)
        && let Some(receipt) =
            reuse_current(&home, &root, options, max_files, paths, &registry_identity)
    {
        let mut receipt = receipt;
        receipt["totalMs"] = json!(started.elapsed().as_millis() as u64);
        return GraphOutput::ok(receipt);
    }
    let scan = Scan {
        max_files,
        cargo,
        registry,
        limits,
    };
    let built = match build_for_ingest(options, paths, security, cancel, &scan) {
        Ok(built) => built,
        Err(output) => return output,
    };
    let build_ms = started.elapsed().as_millis() as u64;
    // A budget the scan's running check cannot bound (projection, encoding)
    // still names the narrower scopes to ingest.
    let too_broad = |mut output: GraphOutput| {
        let files = built.facts.keys().map(String::as_str);
        super::super::graph::scope::hint_narrower_scopes(&mut output.value, &built.root, files);
        output
    };
    let projection = match project_graph(&built, paths, security, cancel, limits) {
        Ok(projection) => projection,
        Err(output) => return too_broad(output),
    };
    let staged = match publication::Staged::new(&home) {
        Ok(staged) => staged,
        Err(error) => {
            return GraphOutput::error(
                ErrorCode::FileAccessFailed,
                format!("cannot stage graph: {error}"),
            );
        }
    };
    let (bytes, digest) =
        match format::write_for_publish(&projection.tables, &staged.graph(), limits, cancel) {
            Ok(encoded) => encoded,
            Err(error) => return too_broad(GraphOutput::error(error.code, error.message)),
        };
    let slug = scope_slug(&built.root, &workspace);
    let (stamp, created_at) = utc_now();
    // Roots whose names map to one slug (`a-b`, `a/b`) still get distinct ids.
    let root_tag = &octocode_engine::digest::sha256(built.root.to_string_lossy().as_bytes())[..6];
    let id = match next_snapshot_id(&home, &format!("{stamp}-{slug}-{root_tag}")) {
        Ok(id) => id,
        Err(error) => return GraphOutput::error(ErrorCode::FileAccessFailed, error),
    };
    let inputs = match settled_inputs(&built, &projection.tables, paths, cancel) {
        Ok(inputs) => inputs,
        Err(output) => return output,
    };
    let manifest = manifest(&ManifestInput {
        id: &id,
        scope: &slug,
        created_at: &created_at,
        workspace: &workspace,
        built: &built,
        projection: &projection,
        bytes,
        digest: &digest,
        max_files,
        exclude_dir: &options.exclude_dir,
        language_globs: &options.language_globs,
        inputs: &inputs,
        build_ms,
        registry,
        registry_identity: &registry_identity,
    });
    if let Err(error) = cancel.check() {
        return GraphOutput::error(ErrorCode::Cancelled, error);
    }
    let dir = match staged.publish(&home, &id, &manifest) {
        Ok(dir) => dir,
        Err(error) => {
            return GraphOutput::error(
                ErrorCode::FileAccessFailed,
                format!(
                    "cannot finish publishing graph {id} at {}: {error}",
                    home.join(&id).display()
                ),
            );
        }
    };
    let checkpoint = super::super::graph::checkpoint::job_path(&home.join(".facts"), &built.root);
    if let Err(error) = super::super::graph::checkpoint::complete(&checkpoint) {
        return GraphOutput::error(
            error.code,
            format!(
                "graph {id} was published at {}, but its extraction checkpoint could not be marked complete: {}",
                dir.display(),
                error.message
            ),
        );
    }
    let pruned = prune(&home, &built.root, options.keep.unwrap_or(DEFAULT_KEEP));
    let mut receipt = json!({
        "id": id,
        "dir": dir.to_string_lossy(),
        "root": built.root.to_string_lossy(),
        "bytes": bytes,
        "sha256": digest,
        "counts": manifest["counts"],
        "calls": manifest["calls"],
        "argumentCoverage": manifest["argumentCoverage"],
        "extraction": manifest["extraction"],
        "linkage": manifest["linkage"],
        "scan": manifest["scan"],
        "buildMs": manifest["buildMs"],
        "totalMs": started.elapsed().as_millis() as u64,
        "next": format!("octocode graph query stats --graph {id} --workspace {}", octocode_io::shell::quote(&workspace.to_string_lossy())),
    });
    if manifest["diagnostics"]["total"].as_u64().unwrap_or(0) > 0 {
        receipt["diagnostics"] = manifest["diagnostics"]["byCode"].clone();
    }
    if !pruned.is_empty() {
        receipt["pruned"] = json!(pruned);
    }
    GraphOutput::ok(receipt)
}

/// Projects `built` into snapshot tables and redacts them. Row counts are
/// checked first: redaction rewrites strings, never rows, so a projection
/// past a bound is refused before that pass.
pub(super) fn project_graph(
    built: &super::super::types::BuiltGraph,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
    limits: &Limits,
) -> Result<tables::Projection, GraphOutput> {
    let files = built.nodes.keys().map(String::as_str).collect::<Vec<_>>();
    let mains = built
        .facts
        .iter()
        .filter(|(_, facts)| {
            facts
                .declarations
                .iter()
                .any(|d| matches!(d.name.as_str(), "main" | "Main") && d.kind != "module")
        })
        .map(|(file, _)| file.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let marks = |file: &str| {
        built
            .facts
            .get(file)
            .map(|facts| facts.marks)
            .unwrap_or_default()
    };
    let project = workspace::Workspace::discover(&built.root, &files, &mains, &marks, &|path| {
        super::super::aliases::read_config_text(paths, security, path)
    });
    let mut projection = match tables::project(built, &project, cancel) {
        Ok(projection) => projection,
        Err(error) => return Err(GraphOutput::error(error.code, error.message)),
    };
    // The build's tally is a lower bound of these rows; one past them would
    // refuse a snapshot that publishes.
    debug_assert!(
        built.tally.nodes <= projection.tables.nodes.len()
            && built.tally.edges <= projection.tables.edges.len(),
        "graph tally {:?} exceeds the projected rows ({} nodes, {} edges)",
        built.tally,
        projection.tables.nodes.len(),
        projection.tables.edges.len()
    );
    if let Err(error) = format::validate_counts(&projection.tables, limits) {
        return Err(GraphOutput::error(ErrorCode::ScopeTooBroad, error));
    }
    redact_secrets(&mut projection.tables, security);
    Ok(projection)
}

fn settled_inputs(
    built: &super::super::types::BuiltGraph,
    tables: &format::GraphTables,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<BTreeMap<String, String>, GraphOutput> {
    let inputs = snapshot_inputs(paths, &built.root, tables);
    if inputs != built.resolution_inputs {
        return Err(GraphOutput::error(
            ErrorCode::ExecutionFailed,
            "project configuration changed during graph analysis; rebuild from settled inputs",
        ));
    }
    for (file, facts) in &built.facts {
        if let Err(error) = cancel.check() {
            return Err(GraphOutput::error(ErrorCode::Cancelled, error));
        }
        let source = match paths.validate_read(built.root.join(file)) {
            Ok(source) => source.canonical,
            Err(_) => {
                return Err(GraphOutput::error(
                    ErrorCode::ExecutionFailed,
                    "source access changed during graph analysis; rebuild from accessible inputs",
                ));
            }
        };
        if source_matches(&source, &facts.digest, facts.stamp.as_ref()) != Some(true) {
            return Err(GraphOutput::error(
                ErrorCode::ExecutionFailed,
                format!(
                    "source {file:?} changed during graph analysis; rebuild from settled inputs"
                ),
            ));
        }
    }
    Ok(inputs)
}

/// How one ingest scans: its file bound, `OCTOCODE_CARGO`, parsers and
/// snapshot bounds.
pub(super) struct Scan<'a> {
    pub max_files: u32,
    pub cargo: Option<&'a str>,
    pub registry: RegistryView<'a>,
    pub limits: &'a Limits,
}

/// The linked graph of `options.path` under the ingest's exclusions and
/// `.gitignore`.
pub(super) fn build_for_ingest(
    options: &IngestOptions,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
    ingest: &Scan<'_>,
) -> Result<super::super::types::BuiltGraph, GraphOutput> {
    let scan = GraphScan {
        checkpoint: graph_home(options.workspace.as_deref())
            .ok()
            .map(|home| home.join(".facts")),
        force: options.force,
        root: options.path.clone(),
        exclude_dir: INGEST_EXCLUDES
            .iter()
            .map(|x| (*x).to_owned())
            .chain(options.exclude_dir.iter().cloned())
            .collect(),
        max_files: ingest.max_files,
        limits: *ingest.limits,
        language_globs: options.language_globs.clone(),
        // Cargo metadata links `crate::`/workspace-crate imports; syntax-only
        // Rust linking would leave them unresolved.
        cargo_metadata: options.path.join("Cargo.toml").is_file(),
        cargo: ingest.cargo.map(str::to_owned),
    };
    // The builder upholds map-index invariants with `expect()`; a corrupt or
    // adversarial input tripping one is an error, not a torn-down host.
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        super::super::graph::build_graph_with_registry(
            &scan,
            paths,
            security,
            cancel,
            ingest.registry,
        )
    })) {
        Ok(Ok(built)) => Ok(built),
        Ok(Err(error)) => Err(GraphOutput {
            class: ExitClass::of(error.code),
            next_query: None,
            value: json!({"error": error.message, "errorCode": error.code, "hints": error.hints}),
        }),
        Err(_) => Err(GraphOutput::error(
            ErrorCode::ExecutionFailed,
            "graph build panicked",
        )),
    }
}

/// Writes the snapshot under a temp dir, renames it into place and points
/// the latest marker at it.
#[cfg(test)]
pub(super) fn publish(
    home: &Path,
    id: &str,
    _stamp: &str,
    bytes: &[u8],
    manifest: &Value,
) -> std::io::Result<PathBuf> {
    let staged = publication::Staged::new(home)?;
    write_atomic(&staged.graph(), bytes, true)?;
    staged.publish(home, id, manifest)
}
