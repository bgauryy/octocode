//! graphBuild: build, redact and publish a snapshot (or reuse the
//! current one).
use super::*;
use crate::tools::ast_graph::limits::{LIMITS, Limits};
use crate::tools::ast_graph::progress;
use octocode_engine::signatures::languages::RegistryView;
#[cfg(test)]
mod determinism_tests;
#[cfg(test)]
mod hooks;
mod publication;
mod redact;
mod settled;
mod snapshot;
#[cfg(test)]
mod tests;
use publication::finish_checkpoint;
#[cfg(test)]
pub(super) use publication::publish;
#[cfg(test)]
pub(super) use redact::PASSES as REDACTIONS;
pub(super) use redact::redact_secrets;
use settled::settled_inputs;
use snapshot::{Publish, publish_snapshot};

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
    let workspace = home
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let registry_identity = match super::super::graph::registry_identity(registry) {
        Ok(identity) => identity,
        Err(error) => return GraphOutput::error(error.code, error.message),
    };
    let root = paths
        .validate(&options.path)
        .ok()
        .map(|valid| valid.canonical);
    let newest = || {
        root.as_deref()
            .and_then(|root| newest_of_root(&home, root))
            .map(|(id, _)| id)
    };
    let reuse = || {
        let root = root.as_deref().filter(|_| !options.refresh)?;
        reuse_current(&home, root, options, paths, &registry_identity)
    };
    // The scope's newest snapshot before building: another one under the
    // lock was published meanwhile, and may make this build redundant.
    let published_before = newest();
    // An unchanged scope skips the build. The probe runs unlocked; under the
    // lock the snapshot must still be the newest (or re-check as current).
    if let Some(probe) = reuse() {
        let lock = match publication::lock(&home, paths, cancel) {
            Ok(lock) => lock,
            Err(error) => return error,
        };
        let still_newest = probe["id"].as_str().map(str::to_owned) == newest();
        if let Some(receipt) = if still_newest { Some(probe) } else { reuse() } {
            return publication::point_latest(&home, receipt, started);
        }
        drop(lock);
    }
    let scan = Scan {
        cargo,
        registry,
        limits,
    };
    #[cfg(test)]
    hooks::reach(hooks::Point::Build, &home);
    // Named like every output path: relative to the workspace.
    progress::emit(|| {
        let scope = paths.workspace_relative(&options.path).unwrap_or_else(|| {
            let within = options.path.strip_prefix(&workspace).ok();
            within.unwrap_or(&options.path).display().to_string()
        });
        format!("scanning {scope}")
    });
    let built = match build_for_ingest(options, paths, security, cancel, &scan) {
        Ok(built) => built,
        Err(output) => return output,
    };
    let build_ms = crate::num::millis_of(started.elapsed());
    let projection = match project_graph(&built, paths, security, cancel, limits) {
        Ok(projection) => projection,
        Err(output) => return snapshot::too_broad(&built, output),
    };
    progress::emit(|| {
        let tables = &projection.tables;
        let (nodes, edges) = (tables.nodes.len(), tables.edges.len());
        format!(
            "linked {} files: {nodes} nodes, {edges} edges",
            built.facts.len()
        )
    });
    let inputs = match settled_inputs(&built, &projection.tables, paths, cancel) {
        Ok(inputs) => inputs,
        Err(output) => return output,
    };
    #[cfg(test)]
    hooks::reach(hooks::Point::Publish, &home);
    // Staging, ids, publication and retention: one process at a time.
    let _lock = match publication::lock(&home, paths, cancel) {
        Ok(lock) => lock,
        Err(error) => return error,
    };
    if newest() != published_before
        && let Some(receipt) = reuse()
    {
        if let Err(error) = finish_checkpoint(options, &home, &built, &receipt) {
            return error;
        }
        return publication::point_latest(&home, receipt, started);
    }
    publish_snapshot(
        &Publish {
            options,
            home: &home,
            workspace: &workspace,
            built: &built,
            projection: &projection,
            inputs: &inputs,
            build_ms,
            registry,
            registry_identity: &registry_identity,
            limits,
        },
        cancel,
        started,
    )
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

/// What a build stopped mid-extraction tells its caller.
pub(super) const RESUME_HINT: &str =
    "extraction committed so far is kept; build this scope again to resume from it";

/// How one ingest scans: `OCTOCODE_CARGO`, parsers and snapshot bounds.
pub(super) struct Scan<'a> {
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
        checkpoint: (!options.memory_only)
            .then(|| {
                graph_home(options.workspace.as_deref())
                    .ok()
                    .map(|home| home.join(".facts"))
            })
            .flatten(),
        refresh: options.refresh,
        root: options.path.clone(),
        exclude_dir: INGEST_EXCLUDES
            .iter()
            .map(|x| (*x).to_owned())
            .chain(options.exclude_dir.iter().cloned())
            .collect(),
        limits: *ingest.limits,
        language_globs: options.language_globs.clone(),
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
        Ok(Err(mut error)) => {
            // Extraction committed before a deadline or a stop is kept:
            // the next build of this scope resumes from it.
            if !options.memory_only
                && matches!(error.code, ErrorCode::Cancelled | ErrorCode::Timeout)
            {
                error.hints.push(RESUME_HINT.to_owned());
            }
            Err(GraphOutput {
                class: ExitClass::of(error.code),
                next: Vec::new(),
                #[cfg(test)]
                validation: Value::Null,
                value: json!({"error": error.message, "errorCode": error.code, "hints": error.hints}),
            })
        }
        Err(_) => Err(GraphOutput::error(
            ErrorCode::ExecutionFailed,
            "graph build panicked",
        )),
    }
}
