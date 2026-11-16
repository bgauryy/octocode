//! Public graph tools: workspace ownership, exact snapshot selection and typed pages.
use super::*;
use crate::execution::{ExecutionContext, ExecutionError};
use crate::response::{outcome::DomainResult, result::Continuation};
use octocode_contract_types::{GraphQueryQuery, GraphQueryStats, GraphQueryStatsOperation};

fn text(row: &Value, key: &str) -> Option<String> {
    row[key].as_str().map(str::to_owned)
}

fn strings(row: &Value, key: &str) -> Vec<String> {
    row[key]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

/// What a graph of a directory outside the workspace needs besides read
/// access: one sentence every refusal of such a path ends with.
const OUTSIDE_WORKSPACE: &str = " A graph outside the workspace also needs local.graphHome (OCTOCODE_GRAPH_HOME) to keep its snapshots, and the directory in allowed paths (ALLOWED_PATHS).";

fn failure(message: impl Into<String>) -> GraphOutput {
    GraphOutput::error(ErrorCode::InvalidInput, message)
}

/// The repair of a graph call on a file, given the file and the snapshot
/// home: the directory to scan or query in its place and the call there.
type Repair<'a> = &'a dyn Fn(&Path, &Path) -> Option<(PathBuf, Value)>;

/// A path as rows name it: from the workspace root, else absolute.
fn shown(paths: &PathPolicy, path: &Path) -> String {
    paths
        .workspace_relative(path)
        .unwrap_or_else(|| path.display().to_string())
}

/// A graph call on a file: graphs scan directories, so the refusal names the
/// directory `repair` chose and `related` is the call there. A configured
/// home inside the file's directory rules that directory out, and the
/// refusal says so. Paths are canonical, as every graph root is: a symlinked
/// file is named by its target's directory.
fn file_scope(
    file: &Path,
    home: &Path,
    configured: bool,
    paths: &PathPolicy,
    repair: Repair<'_>,
) -> GraphOutput {
    let message = "path must be a directory, not a file";
    if configured
        && let Some(dir) = file.parent()
        && home.starts_with(dir)
    {
        return failure(format!(
            "{message}: its directory {} holds local.graphHome ({}), so a graph of it would scan its own snapshots. Use a directory that does not contain it",
            shown(paths, dir),
            home.display()
        ));
    }
    let Some((dir, lead)) = repair(file, home) else {
        return failure(message);
    };
    let mut out = failure(format!(
        "{message}: graphs scan a directory, such as {}",
        shown(paths, &dir)
    ));
    out.value["related"] = json!([lead]);
    out
}

/// graphBuild's repair of a file path: the same build of the file's
/// directory.
fn build_repair(query: &GraphBuildQuery) -> impl Fn(&Path, &Path) -> Option<(PathBuf, Value)> {
    move |file, _| {
        let dir = file.parent()?;
        let query = GraphBuildQuery {
            path: dir.to_str()?.try_into().ok()?,
            ..query.clone()
        };
        Some((
            dir.to_path_buf(),
            Continuation::related_query(query).build(),
        ))
    }
}

/// graphQuery's repair of a file path: the same query of a snapshot that
/// holds the file — the pinned `graph`'s own scope, else the nearest
/// enclosing scope with a snapshot (the newest of it, so the pins go) — and,
/// with no such snapshot, a build of the file's directory.
fn query_repair(row: &Value) -> impl Fn(&Path, &Path) -> Option<(PathBuf, Value)> {
    move |file, home| {
        let root_of = |id: &str| {
            read_snapshot_metadata(&home.join(id))
                .and_then(|manifest| manifest.root)
                .map(PathBuf::from)
        };
        let pinned = row["graph"]
            .as_str()
            .and_then(root_of)
            .filter(|root| file.starts_with(root));
        let scopes = list_snapshots(home)
            .iter()
            .filter_map(|id| root_of(id))
            .collect::<std::collections::HashSet<_>>();
        let enclosing = || {
            file.ancestors()
                .skip(1)
                .find(|dir| scopes.contains(*dir))
                .map(Path::to_path_buf)
        };
        let (dir, keep_pins) = match (pinned, enclosing()) {
            (Some(root), _) => (root, true),
            (None, Some(root)) => (root, false),
            (None, None) => return build_repair(&GraphBuildQuery::default())(file, home),
        };
        let mut row = row.clone();
        row["path"] = json!(dir.to_str()?);
        if !keep_pins && let Some(fields) = row.as_object_mut() {
            fields.remove("graph");
            fields.remove("baseline");
        }
        let query = super::query::typed_row::<GraphQueryQuery>(row)?;
        Some((dir, Continuation::related_query(query).build()))
    }
}

/// The workspace, scanned root and snapshot home of a graph call on `path`.
/// The default home is the workspace's, so the root must lie inside the
/// workspace; a configured home (`local.graphHome`) admits any allowed
/// directory that does not contain it. `repair` is the call's repair of a
/// file path ([`file_scope`]).
fn scope(
    path: &str,
    paths: &PathPolicy,
    configured: Option<&Path>,
    repair: Repair<'_>,
) -> Result<(PathBuf, PathBuf, PathBuf), GraphOutput> {
    let policy = |e: crate::policy::PolicyError| GraphOutput::error(e.shared_code(), e.message);
    let resolve = |path: &str| paths.validate(path).map(|p| p.canonical).map_err(policy);
    let workspace = resolve(".")?;
    // Outside the workspace a graph needs a readable root and a snapshot home
    // of its own: the refusal names both, so one correction makes the call
    // succeed. A missing one leads, as on every local tool, to the tree of its
    // nearest existing parent, where a mistyped name's siblings show.
    let root = paths.validate(path).map(|p| p.canonical).map_err(|mut e| {
        if configured.is_none() && e.code == crate::policy::PolicyErrorCode::OutsideAllowedRoots {
            e.message.push_str(OUTSIDE_WORKSPACE);
        }
        let missing = e.code == crate::policy::PolicyErrorCode::NotFound;
        let mut out = policy(e);
        if missing && let Some(lead) = crate::response::result::missing_path_lead(paths, path) {
            crate::response::result::attach_calls(&mut out.value, lead);
        }
        out
    })?;
    let home = graph_home(Some(&workspace), configured).map_err(failure)?;
    let home = paths.validate_output(home).map_err(policy)?.canonical;
    if !root.is_dir() && (configured.is_some() || root.starts_with(&workspace)) {
        return Err(file_scope(
            &root,
            &home,
            configured.is_some(),
            paths,
            repair,
        ));
    }
    if configured.is_some() {
        if home.starts_with(&root) {
            return Err(failure(format!(
                "local.graphHome ({}) lies inside {}: a build would scan its own snapshots. Build a directory that does not contain it",
                home.display(),
                root.display()
            )));
        }
        return Ok((workspace, root, home));
    }
    if !root.starts_with(&workspace) {
        return Err(GraphOutput::error(
            ErrorCode::OutsideAllowedRoots,
            format!(
                "path must be a directory inside the current workspace ({}): graph snapshots live in the workspace. Start Octocode inside that directory, or use astSearch and localSearch at it.{OUTSIDE_WORKSPACE}",
                workspace.display()
            ),
        ));
    }
    if !home.starts_with(&workspace) {
        return Err(failure("snapshot storage must remain inside the workspace"));
    }
    Ok((workspace, root, home))
}

/// Graph homes other than `home` that hold a snapshot of `root`: the
/// `.octocode/graph` of each allowed ancestor of `root`, where a build run
/// from that workspace put it (the home follows the workspace).
fn other_homes(home: &Path, root: &Path, paths: &PathPolicy) -> Vec<PathBuf> {
    let scanned = root.to_string_lossy();
    root.ancestors()
        .map(|dir| dir.join(".octocode").join("graph"))
        .filter(|candidate| candidate != home && paths.validate(candidate).is_ok())
        .filter(|candidate| {
            list_snapshots(candidate).iter().any(|id| {
                read_snapshot_metadata(&candidate.join(id))
                    .is_some_and(|manifest| manifest.root.as_deref() == Some(scanned.as_ref()))
            })
        })
        .collect()
}

fn missing_snapshot(message: &str, root: &Path, home: &Path, paths: &PathPolicy) -> GraphOutput {
    // Named as a row path is: from the workspace root, else absolute (a
    // configured home, or another workspace's).
    let mut out = GraphOutput::error(
        ErrorCode::NotFound,
        format!("{message} (searched {})", shown(paths, home)),
    );
    let others = other_homes(home, root, paths);
    let mut hints = vec![String::from(
        "Use an existing snapshot ID, or run graphBuild for this path and use its returned ID.",
    )];
    if !others.is_empty() {
        let others = others
            .iter()
            .map(|other| shown(paths, other))
            .collect::<Vec<_>>()
            .join(", ");
        hints.push(format!(
            "Snapshots of this path exist under {others}, the default home of the workspace that built them (<workspace>/.octocode/graph; the workspace follows where Octocode runs). Run Octocode from the directory holding that .octocode, or build here."
        ));
    }
    crate::response::result::set_hints(&mut out.value, hints);
    if let Some(build) = root
        .to_str()
        .and_then(|root| super::query::typed_row::<GraphBuildQuery>(json!({"path": root})))
    {
        out.value["related"] = json!([Continuation::related_query(build).build()]);
    }
    out
}

/// The graphQuery `stats` call for snapshot `id` of `path`.
fn stats_lead(id: &str, path: &str) -> Option<Value> {
    let stats = GraphQueryQuery {
        subtype_0: Some(GraphQueryStats {
            graph: Some(id.try_into().ok()?),
            include_provenance: None,
            main_goal: None,
            operation: GraphQueryStatsOperation::Stats,
            path: Some(path.try_into().ok()?),
            reasoning: None,
            view: None,
        }),
        ..GraphQueryQuery::default()
    };
    Some(Continuation::related_query(stats).build())
}

/// A snapshot ID that a later build of its scope retired: the error names the
/// snapshot that replaced it and leads to it.
fn superseded_snapshot(id: &str, successor: &str, path: &str) -> GraphOutput {
    let mut out = GraphOutput::error(
        ErrorCode::NotFound,
        format!("snapshot {id} was replaced by a newer graphBuild of this path: {successor}"),
    );
    crate::response::result::set_hints(
        &mut out.value,
        [format!(
            "Repeat the query with graph {successor}, or omit graph for the newest snapshot of this path. Offsets and pages of {id} do not carry over to a rebuilt snapshot."
        )],
    );
    if let Some(lead) = stats_lead(successor, path) {
        out.value["related"] = json!([lead]);
    }
    out
}

/// Where graph IDs of one requested scope resolve.
struct Snapshots<'a> {
    paths: &'a PathPolicy,
    home: &'a Path,
    workspace: &'a Path,
    /// The canonical scope root.
    root: &'a Path,
    /// The scope as the caller wrote it.
    path: &'a str,
}

impl Snapshots<'_> {
    fn exact(&self, id: &str) -> Result<String, GraphOutput> {
        let safe = !id.is_empty()
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            && !id.starts_with('.');
        if safe && is_snapshot(&self.home.join(id)) {
            return Ok(self.home.join(id).to_string_lossy().into_owned());
        }
        Err(safe
            .then(|| successor(self.home, self.workspace, self.root, id))
            .flatten()
            .map_or_else(
                || {
                    missing_snapshot(
                        "snapshot ID does not exist in this workspace",
                        self.root,
                        self.home,
                        self.paths,
                    )
                },
                |newer| superseded_snapshot(id, &newer, self.path),
            ))
    }
}

fn options(
    row: &Value,
    paths: &PathPolicy,
    configured: Option<&Path>,
) -> Result<QueryOptions, GraphOutput> {
    let path = row["path"].as_str().unwrap_or(".");
    let (workspace, root, home) = scope(path, paths, configured, &query_repair(row))?;
    let id = match row["graph"].as_str() {
        Some(id) => id.to_owned(),
        None => list_snapshots(&home)
            .into_iter()
            .rev()
            .find(|id| {
                read_snapshot_metadata(&home.join(id))
                    .is_some_and(|m| m.root.as_deref() == Some(root.to_string_lossy().as_ref()))
            })
            .ok_or_else(|| {
                missing_snapshot("no snapshot for this exact path", &root, &home, paths)
            })?,
    };
    let snapshots = Snapshots {
        paths,
        home: &home,
        workspace: &workspace,
        root: &root,
        path: row["path"].as_str().unwrap_or("."),
    };
    let graph = snapshots.exact(&id)?;
    if read_snapshot_metadata(Path::new(&graph))
        .is_none_or(|m| m.root.as_deref() != Some(root.to_string_lossy().as_ref()))
    {
        return Err(failure(
            "snapshot belongs to a different path; use its original scope",
        ));
    }
    let baseline = row["baseline"]
        .as_str()
        .map(|id| snapshots.exact(id))
        .transpose()?;
    Ok(QueryOptions {
        op: text(row, "operation").unwrap_or_default(),
        path: Some(row["path"].as_str().unwrap_or(".").to_owned()),
        include_provenance: row["includeProvenance"] == true,
        workspace: Some(workspace),
        home: configured.map(Path::to_path_buf),
        graph: Some(graph),
        baseline,
        target: text(row, "target").or_else(|| text(row, "cycle")),
        targets: strings(row, "target"),
        reasons: strings(row, "reasons"),
        symbol_name: text(row, "symbolName"),
        to: text(row, "to"),
        edges: strings(row, "edges"),
        kind: text(row, "kind"),
        direction: text(row, "direction"),
        rank_by: text(row, "rankBy"),
        depth: row["depth"]
            .as_u64()
            .map(octocode_engine::num::saturating_u32),
        confidence: text(row, "confidence"),
        limit: row["limit"]
            .as_u64()
            .map(octocode_engine::num::saturating_usize),
        offset: row["offset"]
            .as_u64()
            .map(octocode_engine::num::saturating_usize),
        page: row["page"]
            .as_u64()
            .map(octocode_engine::num::saturating_usize),
        list: text(row, "list"),
        view: text(row, "view"),
        changed: strings(row, "changed"),
        since: text(row, "since"),
        detectors: strings(row, "detectors"),
        entries: strings(row, "entries"),
        min_score: row["minScore"].as_f64(),
        min_tier: row["minTier"]
            .as_u64()
            .and_then(|tier| u8::try_from(tier).ok()),
        check_freshness: true,
        ..Default::default()
    })
}

/// Build measurements no follow-up call takes and no coverage claim rests on,
/// and the call-resolution breakdown (`linkage` is its coverage claim).
const RECEIPT_INTERNALS: [&str; 8] = [
    "dir",
    "bytes",
    "sha256",
    "extraction",
    "argumentCoverage",
    "calls",
    "buildMs",
    "totalMs",
];

fn build(
    query: &GraphBuildQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    context: &ExecutionContext,
    cargo: Option<&str>,
    store: &Store<'_>,
) -> GraphOutput {
    let (workspace, root, _) = match scope(&query.path, paths, store.home, &build_repair(query)) {
        Ok(scope) => scope,
        Err(error) => return error,
    };
    let options = IngestOptions {
        memory_only: store.memory_only,
        workspace: Some(workspace),
        home: store.home.map(Path::to_path_buf),
        ..IngestOptions::of_query(query, root.clone())
    };
    let mut out = ingest(&options, paths, security, context, cargo);
    if let Some(object) = out.value.as_object_mut() {
        // The scope sits inside the workspace: named relative to it, like every
        // local evidence path.
        if let Some(relative) = object
            .get("root")
            .and_then(Value::as_str)
            .and_then(|path| paths.workspace_relative(path))
        {
            object.insert("root".into(), Value::String(relative));
        }
        // What a follow-up call or a coverage claim needs stays; the rest of
        // the build's measurements live in the snapshot (graphQuery stats).
        for key in RECEIPT_INTERNALS {
            object.remove(key);
        }
        if let Some(scan) = object.get_mut("scan") {
            *scan = super::manifest::scan_disclosure(scan);
        }
    }
    if let Some(id) = out.value["id"].as_str()
        && let Some(lead) = stats_lead(id, &query.path)
    {
        out.value["related"] = json!([lead]);
    }
    if let Err(error) = skipped::note_skipped(query, &root, paths, context, &mut out.value) {
        return GraphOutput::error(error.code, error.message);
    }
    resume_paused(&mut out, query);
    retry_after_change(&mut out, query);
    out
}

/// A build that lost a race with an edit: the hint says the failure is
/// transient and `related` is the same build, runnable as written.
fn retry_after_change(out: &mut GraphOutput, query: &GraphBuildQuery) {
    if !out.retry {
        return;
    }
    crate::response::result::set_hints(
        &mut out.value,
        [
            "A file changed while the graph was being built, so no snapshot was published. This is not a tool failure: run the same graphBuild again once the edits settle."
                .to_owned(),
        ],
    );
    out.value["related"] = json!([Continuation::related_query(query.clone()).build()]);
}

/// A paused build's `next.expandScan`: the same request, never a refresh,
/// which would discard the checkpoints it resumes from.
fn resume_paused(out: &mut GraphOutput, query: &GraphBuildQuery) {
    if out.value.get("progress").is_some() {
        let resume = GraphBuildQuery {
            refresh: false,
            ..query.clone()
        };
        crate::response::result::set_next(
            &mut out.value,
            "expandScan",
            Continuation::continued_query(resume).build(),
        );
    }
}

/// Where graph tools keep their state: the runtime's storage settings.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Store<'a> {
    /// `storage.mode: memory`: extraction checkpoints stay in memory; the
    /// requested snapshot is still published.
    pub memory_only: bool,
    /// `local.graphHome`: the snapshot home in place of the workspace's.
    pub home: Option<&'a Path>,
}

/// One validated graphBuild row: ingest a snapshot of its path.
pub(crate) fn execute_build(
    query: &GraphBuildQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    context: &ExecutionContext,
    cargo: Option<&str>,
    store: &Store<'_>,
) -> Result<DomainResult, ExecutionError> {
    context.check()?;
    let out = build(query, paths, security, context, cargo, store);
    context.check()?;
    Ok(result(out))
}

/// One validated graphQuery row against an exact snapshot of its path.
pub(crate) fn execute_query(
    row: &Value,
    paths: &PathPolicy,
    context: &ExecutionContext,
    rows: Option<std::sync::Arc<SavedRows>>,
    home: Option<&Path>,
) -> Result<DomainResult, ExecutionError> {
    context.check()?;
    let mut out = match options(row, paths, home) {
        Ok(options) => {
            let unrecorded = record_reads(&options);
            let mut out = super::query::query_cancellable(
                &QueryOptions {
                    response_window: context.response_window,
                    rows,
                    ..options
                },
                paths,
                context,
            );
            if let Some(hint) = unrecorded {
                crate::response::paging::push_warnings(&mut out.value, [hint]);
            }
            out
        }
        Err(error) => error,
    };
    context.check()?;
    pages::convert(&mut out, row);
    Ok(result(out))
}

/// Renews the lease of every snapshot `options` reads, so a rebuild's
/// retention leaves them for the walk in progress. A read that cannot be
/// recorded still answers, and says so.
fn record_reads(options: &QueryOptions) -> Option<String> {
    let failures = [options.graph.as_deref(), options.baseline.as_deref()]
        .into_iter()
        .flatten()
        .filter_map(|dir| {
            renew_lease(Path::new(dir))
                .err()
                .map(|error| format!("{dir}: {error}"))
        })
        .collect::<Vec<_>>();
    (!failures.is_empty()).then(|| {
        format!(
            "This read could not be recorded ({}); a graphBuild of this path may remove the snapshot before the walk ends.",
            failures.join("; ")
        )
    })
}

fn result(out: GraphOutput) -> DomainResult {
    let status = match out.class {
        ExitClass::Success => None,
        ExitClass::Empty => Some("empty"),
        _ => Some("error"),
    };
    DomainResult::payload(out.value, status)
}

mod pages;

mod skipped;
#[cfg(test)]
mod tests;
