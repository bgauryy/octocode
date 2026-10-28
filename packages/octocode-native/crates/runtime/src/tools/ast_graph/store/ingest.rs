//! `graph ingest`: build, redact and publish a snapshot (or reuse the
//! current one).
use super::*;
use octocode_engine::signatures::languages::RegistryView;

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
    )
}

pub(super) fn ingest_with_registry(
    options: &IngestOptions,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
    cargo: Option<&str>,
    registry: RegistryView<'_>,
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
    let workspace = home
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let max_files = options.max_files.unwrap_or(DEFAULT_MAX_FILES);
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
    let built = match build_for_ingest(options, paths, security, cancel, max_files, cargo, registry)
    {
        Ok(built) => built,
        Err(output) => return output,
    };
    let build_ms = started.elapsed().as_millis() as u64;
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
    let mut projection = match tables::project(&built, &project, cancel) {
        Ok(projection) => projection,
        Err(error) => return GraphOutput::error(error.code, error.message),
    };
    redact_secrets(&mut projection.tables, security);
    let (bytes, digest) = match format::encode_for_publish(&projection.tables, cancel) {
        Ok(encoded) => encoded,
        Err(error) => return GraphOutput::error(error.code, error.message),
    };
    if bytes.len() > MAX_GRAPH_BYTES {
        return GraphOutput::error(
            ErrorCode::ScopeTooBroad,
            format!(
                "encoded graph exceeds the {MAX_GRAPH_BYTES}-byte snapshot limit; ingest a narrower scope"
            ),
        );
    }

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
        bytes: bytes.len(),
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
    let dir = match publish(&home, &id, &stamp, &bytes, &manifest) {
        Ok(dir) => dir,
        Err(error) => {
            return GraphOutput::error(
                ErrorCode::FileAccessFailed,
                format!("cannot publish graph: {error}"),
            );
        }
    };
    let pruned = prune(&home, &built.root, options.keep.unwrap_or(DEFAULT_KEEP));
    let mut receipt = json!({
        "id": id,
        "dir": dir.to_string_lossy(),
        "root": built.root.to_string_lossy(),
        "bytes": bytes.len(),
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

/// The linked graph of `options.path` under the ingest's exclusions and
/// `.gitignore`.
pub(super) fn build_for_ingest(
    options: &IngestOptions,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
    max_files: u32,
    cargo: Option<&str>,
    registry: RegistryView<'_>,
) -> Result<super::super::types::BuiltGraph, GraphOutput> {
    let scan = GraphScan {
        fact_cache: (!options.force)
            .then(|| {
                graph_home(options.workspace.as_deref())
                    .ok()
                    .map(|home| home.join(".facts"))
            })
            .flatten(),
        root: options.path.clone(),
        exclude_dir: INGEST_EXCLUDES
            .iter()
            .map(|x| (*x).to_owned())
            .chain(options.exclude_dir.iter().cloned())
            .collect(),
        max_files,
        language_globs: options.language_globs.clone(),
        // Cargo metadata links `crate::`/workspace-crate imports; syntax-only
        // Rust linking would leave them unresolved.
        cargo_metadata: options.path.join("Cargo.toml").is_file(),
        cargo: cargo.map(str::to_owned),
    };
    // The builder upholds map-index invariants with `expect()`; a corrupt or
    // adversarial input tripping one is an error, not a torn-down host.
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        super::super::graph::build_graph_with_registry(&scan, paths, security, cancel, registry)
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

/// Bytes of joined strings one secret scan reads. The scanner takes bounded
/// text, and most snapshots hold no secret, so a clean batch is cleared in
/// one scan.
pub(super) const SECRET_SCAN_BATCH: usize = 1 << 20;

/// Redacts every string of `tables` the secret scanner flags (a token used as
/// an import specifier is a package name; a diagnostic can quote source), so
/// no secret of the source reaches the disk, then re-sorts the lookups the
/// new strings reorder. Digest strings are hashes the staleness check
/// compares, never source text, so they are kept as they are.
pub(super) fn redact_secrets(tables: &mut format::GraphTables, security: &ContentSecurity) {
    for source in &mut tables.sources.descriptors.snapshots {
        source.path = security.sanitize_text(&source.path, None).content;
    }
    let digests = tables
        .digests
        .iter()
        .map(|(_, string)| *string as usize)
        .collect::<std::collections::HashSet<_>>();
    let strings = &tables.strings;
    let mut batches = Vec::new();
    let mut start = 0;
    while start < strings.len() {
        let mut end = start;
        let mut size = 0;
        while end < strings.len() && (end == start || size < SECRET_SCAN_BATCH) {
            size += strings[end].len() + 1;
            end += 1;
        }
        batches.push(start..end);
        start = end;
    }
    // Batches are independent: scan them on every core.
    let next = std::sync::atomic::AtomicUsize::new(0);
    let redacted = std::thread::scope(|scope| {
        let workers = (0..crate::tools::workers::workers(None, batches.len(), None))
            .map(|_| {
                scope.spawn(|| {
                    let mut redacted = Vec::new();
                    while let Some(batch) =
                        batches.get(next.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
                    {
                        let scanned = batch.clone().filter(|index| !digests.contains(index));
                        let text = scanned
                            .clone()
                            .map(|index| strings[index].as_str())
                            .collect::<Vec<_>>()
                            .join("\n");
                        if security.sanitize_text(&text, None).has_secrets {
                            redacted.extend(scanned.filter_map(|index| {
                                let result = security.sanitize_text(&strings[index], None);
                                result.has_secrets.then_some((index, result.content))
                            }));
                        }
                    }
                    redacted
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .flat_map(|worker| {
                // A failed scan must not publish unredacted strings.
                worker
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            })
            .collect::<Vec<_>>()
    });
    if redacted.is_empty() {
        return;
    }
    for (index, content) in redacted {
        tables.strings[index] = content;
    }
    tables.index();
}

/// Writes the snapshot under a temp dir, renames it into place and points
/// the latest marker at it.
pub(super) fn publish(
    home: &Path,
    id: &str,
    stamp: &str,
    bytes: &[u8],
    manifest: &Value,
) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(home)?;
    let tmp = home.join(format!(".tmp-{}-{stamp}", std::process::id()));
    std::fs::create_dir_all(&tmp)?;
    std::fs::write(tmp.join(GRAPH_FILE), bytes)?;
    let text = serde_json::to_vec_pretty(manifest).unwrap_or_default();
    std::fs::write(tmp.join(MANIFEST_FILE), text)?;
    let target = home.join(id);
    std::fs::rename(&tmp, &target)?;
    write_atomic(&home.join(LATEST_FILE), id.as_bytes(), false)?;
    Ok(target)
}
