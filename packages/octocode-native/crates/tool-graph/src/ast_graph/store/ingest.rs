//! graphBuild: extract, build, redact and publish a snapshot, or answer an
//! unchanged scope with its current one.
use super::*;
use crate::ast_graph::limits::{LIMITS, Limits};
use crate::ast_graph::progress;
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
mod unchanged;
use publication::finish_checkpoint;
#[cfg(test)]
pub(super) use publication::publish;
#[cfg(test)]
pub(super) use redact::PASSES as REDACTIONS;
#[cfg(test)]
pub(super) use redact::SCANNED as SCANNED_STRINGS;
pub(super) use redact::redact_secrets;
#[cfg(test)]
pub(super) use redact::{Fault, inject_redact_fault};
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

/// [`ingest`](fn@ingest) with the parser registry and the snapshot bounds it builds
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
    let home = match graph_home(options.workspace.as_deref(), options.home.as_deref()) {
        Ok(home) => home,
        Err(message) => return GraphOutput::error(ErrorCode::WorkspaceRootInvalid, message),
    };
    let home = match paths.validate_output(&home) {
        Ok(valid) => valid.canonical,
        Err(error) => {
            return GraphOutput::error(error.shared_code(), error.message);
        }
    };
    // The workspace names the scope, wherever its home is.
    let workspace = options
        .workspace
        .as_deref()
        .map(|workspace| std::fs::canonicalize(workspace).unwrap_or_else(|_| workspace.into()))
        .unwrap_or_default();
    let registry_identity = match super::super::graph::registry_identity(registry) {
        Ok(identity) => identity,
        Err(error) => return GraphOutput::error(error.code, error.message),
    };
    let build_identity = unchanged::build_identity(options, cargo, &registry_identity);
    let identity = unchanged::Identity {
        build: &build_identity,
        registry: &registry_identity,
    };
    let judged = match unchanged::reuse(options, &home, paths, &identity, cancel, started) {
        unchanged::Reuse::Current(receipt) => return receipt,
        unchanged::Reuse::Build { judged } => judged,
    };
    let scanned = judged
        .as_deref()
        .and_then(|id| redact::scanned_strings(&home.join(id), &build_identity, cancel));
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
    let mut built = match build_for_ingest(options, paths, security, cancel, &scan) {
        Ok(built) => built,
        Err(output) => return output,
    };
    if let Some(pause) = built.paused {
        return snapshot::paused(&built.root, pause);
    }
    let build_ms = octocode_runtime_base::num::millis_of(started.elapsed());
    let projection = match project_graph(
        &mut built,
        options,
        (paths, security),
        scanned.as_ref().map(<[format::Strings; 2]>::as_slice),
        cancel,
        limits,
    ) {
        Ok(projection) => projection,
        Err(output) => {
            let files = built.facts.keys().map(String::as_str);
            return snapshot::too_broad(
                &built.root,
                &crate::ast_graph::build_scope::BuildScope::of_options(options),
                files,
                output,
            );
        }
    };
    progress::emit(|| {
        let tables = &projection.tables;
        let (nodes, edges) = (tables.nodes.len(), tables.edges.len());
        format!(
            "linked {}: {nodes} nodes, {edges} edges",
            octocode_engine::text::counted(built.facts.len(), "file", "files")
        )
    });
    #[cfg(test)]
    hooks::reach(hooks::Point::Settle, &home);
    let settled = match settled_inputs(&built, &projection.tables, paths, cancel) {
        Ok(settled) => settled,
        Err(output) => return output,
    };
    let extensions =
        registry.extensions(octocode_engine::text::analysis::AnalysisOperation::GraphFacts);
    let scope_proof = settled_scope_proof(
        &built.root,
        paths.global_ignore(),
        &projection.tables,
        |dir| {
            scope_files(
                paths,
                &built.root,
                &crate::ast_graph::build_scope::BuildScope::of_options(options),
                built.file_scope.as_deref(),
                &extensions,
                dir,
            )
        },
    );
    let argument_coverage = argument_coverage::coverage(&built, &projection.tables);
    // The tables hold everything publication reads of the linked facts, the
    // largest part of a build: they are freed before encoding, keeping only
    // the scanned files' names.
    let files = std::mem::take(&mut built.facts)
        .into_keys()
        .collect::<Vec<_>>();
    #[cfg(test)]
    hooks::reach(hooks::Point::Publish, &home);
    // Staging, ids, publication and retention: one process at a time.
    let _lock = match publication::lock(&home, paths, cancel) {
        Ok(lock) => lock,
        Err(error) => return error,
    };
    if let Some((id, receipt)) = unchanged::published_meanwhile(
        options,
        &home,
        paths,
        &identity,
        judged.as_deref(),
        cancel,
        started,
    ) {
        let published = json!({"id": id, "dir": home.join(&id)});
        return match finish_checkpoint(options, &home, &built, &published) {
            Ok(()) => receipt,
            Err(error) => error,
        };
    }
    publish_snapshot(
        &Publish {
            options,
            home: &home,
            workspace: &workspace,
            built: &built,
            files: &files,
            projection: &projection,
            argument_coverage: &argument_coverage,
            inputs: &settled.inputs,
            input_proof: settled.proof.as_deref(),
            changed: (settled.changed > 0).then(|| snapshot::Changed {
                files: settled.changed,
                path: paths
                    .workspace_relative(&options.path)
                    .unwrap_or_else(|| options.path.display().to_string()),
                rebuild: same_build(&options.path, options, paths),
            }),
            scope_proof: scope_proof.as_ref(),
            build_ms,
            registry,
            registry_identity: &registry_identity,
            build_identity: &build_identity,
            limits,
        },
        cancel,
        started,
    )
}

/// Projects `built` into snapshot tables and redacts them. Row counts are
/// checked first: redaction rewrites strings, never rows, so a projection
/// past a bound is refused before that pass. The build's captured sources
/// move into the tables ([`tables::project`]). `scanned`: strings a
/// snapshot built with the same inputs stores, which the secret scan has
/// already read ([`redact_secrets`]).
pub(super) fn project_graph(
    built: &mut super::super::types::BuiltGraph,
    options: &IngestOptions,
    (paths, security): (&PathPolicy, &ContentSecurity),
    scanned: Option<&[format::Strings]>,
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
    // The tables now hold what the linked facts said. They are released here
    // rather than at publication because the secret scan's regexes and their
    // per-thread caches come on top.
    built
        .facts
        .values_mut()
        .for_each(super::super::types::FileFacts::release_projected);
    built.nodes.clear();
    if let Err(unscanned) = redact_secrets(&mut projection.tables, security, scanned) {
        return Err(unredacted(&unscanned, &built.root, options, paths));
    }
    Ok(projection)
}

/// The failure of a build whose secret scan unwound: the files whose string
/// unwound alone, and the same build without them (each anchored at the
/// scope root, its glob characters escaped) as a related graphBuild call.
fn unredacted(
    unscanned: &redact::Unscanned,
    root: &Path,
    options: &IngestOptions,
    paths: &PathPolicy,
) -> GraphOutput {
    let panic = &unscanned.panic;
    let files = &unscanned.files;
    let on = if files.is_empty() {
        String::new()
    } else {
        format!(" on a string of {}", files.join(", "))
    };
    let mut out = GraphOutput::error(
        ErrorCode::ExecutionFailed,
        format!(
            "graph build failed redacting the snapshot: {panic}{on}; no snapshot was published"
        ),
    );
    if let Some(retry) = exclude_retry(files, root, options, paths) {
        out.value["related"] = json!([retry]);
    }
    out
}

/// The same build without `files` (root-relative), each excluded as that one
/// file: its [`literal`](octocode_engine::search::glob::literal) glob
/// anchored at the scope root. `None` without files.
fn exclude_retry(
    files: &[String],
    root: &Path,
    options: &IngestOptions,
    paths: &PathPolicy,
) -> Option<Value> {
    if files.is_empty() {
        return None;
    }
    let mut scope = crate::ast_graph::build_scope::BuildScope::of_options(options);
    scope.exclude.extend(
        files
            .iter()
            .map(|file| format!("/{}", octocode_engine::search::glob::literal(file))),
    );
    build_lead(root, &scope, paths)
}

/// The same build of `root`, as a related graphBuild call: a later run reads
/// what changed since this one.
fn same_build(root: &Path, options: &IngestOptions, paths: &PathPolicy) -> Option<Value> {
    build_lead(
        root,
        &crate::ast_graph::build_scope::BuildScope::of_options(options),
        paths,
    )
}

/// A related graphBuild call of `root` with `scope`'s walk and exclusions.
fn build_lead(
    root: &Path,
    scope: &crate::ast_graph::build_scope::BuildScope,
    paths: &PathPolicy,
) -> Option<Value> {
    let path = paths
        .workspace_relative(root)
        .unwrap_or_else(|| root.display().to_string());
    let mut row = json!({"path": path});
    scope.write_row(&mut row);
    super::query::typed_row::<GraphBuildQuery>(row).map(|retry| {
        octocode_runtime_base::response::result::Continuation::related_query(retry).build()
    })
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
                graph_home(options.workspace.as_deref(), options.home.as_deref())
                    .ok()
                    .map(|home| home.join(".facts"))
            })
            .flatten(),
        refresh: options.refresh,
        root: options.path.clone(),
        exclude: options.exclude.clone(),
        walk: options.walk,
        limits: *ingest.limits,
        language_globs: options.language_globs.clone(),
        cargo: ingest.cargo.map(str::to_owned),
    };
    // The builder upholds map-index invariants with `expect()`; a corrupt or
    // adversarial input tripping one is an error, not a torn-down host.
    // Each file's extraction and linking contain their own panics; this is
    // the build's own, named by where it panicked.
    match octocode_engine::contain::catch(octocode_engine::contain::Phase::Link, || {
        super::super::graph::build_graph_with_registry(
            &scan,
            paths,
            security,
            cancel,
            ingest.registry,
        )
    }) {
        Ok(Ok(built)) => Ok(built),
        Ok(Err(mut error)) => {
            let torn = error.calls.take().and_then(|calls| {
                calls[super::super::graph::TORN_FILE]
                    .as_str()
                    .map(str::to_owned)
            });
            // Extraction committed before a deadline or a stop is kept:
            // the next build of this scope resumes from it.
            if !options.memory_only
                && matches!(error.code, ErrorCode::Cancelled | ErrorCode::Timeout)
            {
                error.hints.push(RESUME_HINT.to_owned());
            }
            let mut value = json!({"error": error.message, "errorCode": error.code});
            octocode_runtime_base::response::result::add_hints(&mut value, error.hints);
            if let Some(retry) =
                torn.and_then(|file| exclude_retry(&[file], &options.path, options, paths))
            {
                value["related"] = json!([retry]);
            }
            Err(GraphOutput {
                class: ExitClass::of(error.code),
                next: Vec::new(),
                #[cfg(test)]
                validation: Box::default(),
                value,
            })
        }
        Err(panic) => Err(GraphOutput::error(
            ErrorCode::ExecutionFailed,
            match panic.site {
                Some(site) => format!("graph build panicked at {site}"),
                None => "graph build panicked".to_owned(),
            },
        )),
    }
}
