//! Change assessment: impact (optionally since a git revision), issues,
//! drift and baselines.
use super::*;
use crate::tools::ast_graph::build_scope::BuildScope;

mod changed;

pub(super) use changed::git;
use changed::git_changed;

impl Query<'_> {
    pub(super) fn impact(&self) -> Result<GraphOutput, GraphOutput> {
        let (extra, page) = self.complete_rows(|| self.impact_rows())?;
        Ok(run_page(self.options, self.g, extra, page))
    }

    /// The blast radius's complete rows and the facts its first page states.
    fn impact_rows(&self) -> Result<(Map<String, Value>, Vec<Value>), GraphOutput> {
        let (g, options) = (self.g, self.options);
        let root = PathBuf::from(g.manifest["root"].as_str().unwrap_or_default());
        let mut refs = options.target.iter().cloned().collect::<Vec<_>>();
        refs.extend(
            options
                .changed
                .iter()
                .flat_map(|c| c.split(','))
                .map(str::trim)
                .filter(|c| !c.is_empty())
                .map(str::to_owned),
        );
        let mut global = Vec::new();
        let mut unmapped = Vec::new();
        if let Some(rev) = &options.since {
            let changed = git_changed(&root, rev, self.cancel)?;
            let output = self.graph_output();
            for (path, change) in changed {
                // The snapshots this graph lives in are the tool's output,
                // not a change to the code.
                if output
                    .as_deref()
                    .is_some_and(|output| canonical(&root.join(&path)).starts_with(output))
                {
                    continue;
                }
                if super::super::super::freshness::is_global_config(&path) {
                    global.push((
                        crate::tools::ast_graph::paths::parent(&path).to_owned(),
                        path,
                    ));
                } else if g.t.by_key(&path).is_some() {
                    refs.push(path);
                } else {
                    unmapped.push(json!({"path": path, "status": change.as_str()}));
                }
            }
        }
        if refs.is_empty() && global.is_empty() {
            let Some(rev) = &options.since else {
                return Err(GraphOutput::error(
                    ErrorCode::InvalidInput,
                    "`impact` needs a target, changed, or since",
                ));
            };
            // A change set with nothing in the graph is an empty answer.
            let mut summary = json!({
                "since": rev,
                "reason": if unmapped.is_empty() {
                    format!("no files changed since {rev}")
                } else {
                    format!("no file changed since {rev} is in the graph")
                },
            });
            if !unmapped.is_empty() {
                summary["unmapped"] = json!(unmapped);
            }
            let mut extra = Map::new();
            extra.insert("summary".into(), summary);
            return Ok((extra, Vec::new()));
        }
        let mut seeds = Vec::new();
        for reference in &refs {
            seeds.push(self.resolve(reference, self.flags.kind)?);
        }
        seeds.sort_unstable();
        seeds.dedup();
        let max_depth = self.flags.depth.unwrap_or(DEFAULT_IMPACT_DEPTH);
        let impact = super::super::impact::run(&g.t, &seeds, &global, max_depth);
        let mut extra = Map::new();
        let mut summary = impact.summary;
        summary["maxDepth"] = json!(max_depth);
        if let Some(first) = seeds.first() {
            extra.insert("coverage".into(), g.coverage(*first));
        }
        if !unmapped.is_empty() {
            summary["unmapped"] = json!(unmapped);
        }
        extra.insert("summary".into(), summary);
        Ok((extra, impact.rows))
    }

    pub(super) fn issues(&self, paths: &PathPolicy) -> Result<GraphOutput, GraphOutput> {
        let (extra, page) = self.complete_rows(|| self.issue_rows(paths))?;
        Ok(run_page(self.options, self.g, extra, page))
    }

    /// The findings' complete rows, detectors run once, and the facts the
    /// first page states.
    fn issue_rows(
        &self,
        paths: &PathPolicy,
    ) -> Result<(Map<String, Value>, Vec<Value>), GraphOutput> {
        let (g, options) = (self.g, self.options);
        let detectors = options
            .detectors
            .iter()
            .flat_map(|d| d.split(','))
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let invalid = |message: String| GraphOutput::error(ErrorCode::InvalidInput, message);
        let root = PathBuf::from(g.manifest["root"].as_str().unwrap_or_default());
        let scan_root = root.is_dir().then_some(root.as_path());
        let mut entries = Vec::new();
        for reference in options
            .entries
            .iter()
            .flat_map(|e| e.split(','))
            .map(str::trim)
            .filter(|e| !e.is_empty())
        {
            let file = self.resolve(reference, Some(NodeKind::File))?;
            entries.push(g.key(file).to_owned());
        }
        let entries = (!entries.is_empty()).then_some(entries.as_slice());
        let stopped = || self.cancel.check().is_err();
        let report =
            super::super::detect::kept_run(&g.id, &g.t, &detectors, scan_root, entries, &stopped);
        self.cancel
            .check()
            .map_err(|message| GraphOutput::error(ErrorCode::Cancelled, message))?;
        let report = report.map_err(invalid)?;
        let min_tier = options.min_tier.unwrap_or(DEFAULT_MIN_TIER);
        let above = package_above(
            &root,
            g.manifest["workspace"].as_str(),
            g.t.components.is_empty(),
            &detectors,
            &g.manifest["scan"],
        );
        // Below its package root a dead-code finding claims at most tier 60.
        let tier = |f: &super::super::detect::Finding| {
            if above.is_some() && SCOPE_JUDGED.contains(&f.detector) {
                f.tier.min(60)
            } else {
                f.tier
            }
        };
        let keep = |f: &&super::super::detect::Finding| {
            options.min_score.is_none_or(|min| f.score >= min) && tier(f) >= min_tier
        };
        let total_before = report.findings.len();
        let findings = report
            .findings
            .iter()
            .filter(keep)
            .map(|finding| super::super::detect::Finding {
                tier: tier(finding),
                ..finding.clone()
            })
            .collect::<Vec<_>>();
        let mut extra = Map::new();
        let mut summary = report.summary.clone();
        if let Some(above) = &above
            && let Some(notes) = summary["notes"].as_array_mut()
        {
            notes.push(above.note.clone());
        }
        summary["minTier"] = json!(min_tier);
        let hidden = total_before - findings.len();
        summary["hiddenBelowTier"] = json!(hidden);
        if hidden > 0
            && let Some(notes) = summary["notes"].as_array_mut()
        {
            // byDetector counts before this filter, so its sum is not the
            // page's pagination.total: say by how much and why.
            let filters = options.min_score.map_or_else(
                || format!("minTier {min_tier}"),
                |score| format!("minTier {min_tier} and minScore {score}"),
            );
            notes.push(json!({
                "filter": "minTier",
                "explanation": format!(
                    "{filters} hid hiddenBelowTier findings: byDetector counts them, so its sum is pagination.total plus hiddenBelowTier. Lower them to list the rest."
                ),
            }));
        }
        let mut rows = findings
            .iter()
            .map(super::super::detect::Finding::to_json)
            .collect::<Vec<_>>();
        let constants = state_detector_constants(&mut rows);
        if let Some(notes) = summary["notes"].as_array_mut() {
            notes.extend(
                constants
                    .into_iter()
                    .map(|(detector, shared)| json!({"detector": detector, "shared": shared})),
            );
        }
        let mut leads = above
            .map(|above| above.build)
            .into_iter()
            .collect::<Vec<_>>();
        leads.extend(reexport_leads(&mut summary, options, &g.id));
        if !leads.is_empty() {
            extra.insert("related".into(), Value::Array(leads));
        }
        extra.insert("summary".into(), summary);
        let mut status = BTreeMap::<String, &str>::new();
        if let Some(base) = &options.baseline {
            // Every head finding, listed or hidden by `minTier`/`minScore`: a
            // finding the view hides is still present, so never resolved.
            let baseline = self.baseline(
                paths,
                base,
                &detectors,
                entries,
                &report.findings,
                &mut status,
            )?;
            extra.insert("baseline".into(), baseline);
        }
        for (row, finding) in rows.iter_mut().zip(&findings) {
            if let Some(state) = status.get(&finding.id) {
                row["status"] = json!(state);
            }
        }
        Ok((extra, rows))
    }

    /// The directory holding this graph's snapshots, canonical: the snapshot
    /// directory's parent when `graph` named it by path, else the home.
    fn graph_output(&self) -> Option<PathBuf> {
        let home = match self.g.explicit_path.as_deref() {
            Some(snapshot) => snapshot.parent()?.to_path_buf(),
            None => graph_home(
                self.options.workspace.as_deref(),
                self.options.home.as_deref(),
            )
            .ok()?,
        };
        Some(canonical(&home))
    }

    /// The structural drift from the `baseline` snapshot to this one.
    pub(super) fn drift(&self, paths: &PathPolicy) -> Result<GraphOutput, GraphOutput> {
        let (extra, page) = self.complete_rows(|| self.drift_rows(paths))?;
        Ok(run_page(self.options, self.g, extra, page))
    }

    /// The drift's complete rows and the facts its first page states.
    fn drift_rows(
        &self,
        paths: &PathPolicy,
    ) -> Result<(Map<String, Value>, Vec<Value>), GraphOutput> {
        let base = self.options.baseline.as_deref().ok_or_else(|| {
            GraphOutput::error(
                ErrorCode::InvalidInput,
                "`drift` needs a baseline snapshot id",
            )
        })?;
        let base_options = QueryOptions {
            graph: Some(base.to_owned()),
            workspace: self.options.workspace.clone(),
            home: self.options.home.clone(),
            ..Default::default()
        };
        let base_graph = load(&base_options, paths, self.cancel).map_err(|failure| *failure)?;
        let drift = super::super::drift::diff(&base_graph.t, &self.g.t);
        let mut extra = Map::new();
        extra.insert("baseline".into(), base_graph.summary());
        extra.insert("summary".into(), drift.summary);
        Ok((extra, drift.rows))
    }

    /// Findings compared with a baseline graph: each current one is `new`
    /// or `existing` (written to `status`), and the resolved ones (in the
    /// baseline, absent from `findings`) are listed. `findings` is every head
    /// finding, before the view's filters: resolved means gone, not hidden.
    pub(super) fn baseline(
        &self,
        paths: &PathPolicy,
        base: &str,
        detectors: &[String],
        entries: Option<&[String]>,
        findings: &[super::super::detect::Finding],
        status: &mut BTreeMap<String, &str>,
    ) -> Result<Value, GraphOutput> {
        let base_options = QueryOptions {
            graph: Some(base.to_owned()),
            workspace: self.options.workspace.clone(),
            home: self.options.home.clone(),
            ..Default::default()
        };
        let base_graph = load(&base_options, paths, self.cancel).map_err(|failure| *failure)?;
        let base_root = PathBuf::from(base_graph.manifest["root"].as_str().unwrap_or_default());
        let before = super::super::detect::run(
            &base_graph.t,
            detectors,
            base_root.is_dir().then_some(base_root.as_path()),
            entries,
            &|| self.cancel.check().is_err(),
        );
        self.cancel
            .check()
            .map_err(|message| GraphOutput::error(ErrorCode::Cancelled, message))?;
        let before =
            before.map_err(|message| GraphOutput::error(ErrorCode::InvalidInput, message))?;
        let old = before
            .findings
            .iter()
            .map(|f| f.id.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let now = findings
            .iter()
            .map(|f| f.id.clone())
            .collect::<std::collections::BTreeSet<_>>();
        for finding in findings {
            status.insert(
                finding.id.clone(),
                if old.contains(&finding.id) {
                    "existing"
                } else {
                    "new"
                },
            );
        }
        let resolved = before
            .findings
            .iter()
            .filter(|f| !now.contains(&f.id))
            .map(|f| json!({"id": f.id, "detector": f.detector, "subject": f.subject, "title": f.title}))
            .collect::<Vec<_>>();
        Ok(json!({
            "graph": base_graph.id,
            "new": status.values().filter(|s| **s == "new").count(),
            "existing": status.values().filter(|s| **s == "existing").count(),
            "resolved": resolved.len(),
            "resolvedFindings": resolved,
        }))
    }
}

/// `path` with its deepest existing ancestor resolved (symlinks, `/var` as
/// `/private/var`): a deleted file still compares by its directory.
fn canonical(path: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut at = path;
    loop {
        if let Ok(resolved) = at.canonicalize() {
            return rest
                .iter()
                .rev()
                .fold(resolved, |path, part| path.join(part));
        }
        match (at.parent(), at.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_owned());
                at = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// Dead-code detectors whose verdict rests on what the snapshot holds: the
/// package's entries and the text the mention scan reads.
const SCOPE_JUDGED: &[&str] = &[
    "unreachable-file",
    "unused-export",
    "export-only-local",
    "test-only",
];

/// A snapshot below its package root: the note that says what it left
/// unread, and the build of that root.
struct PackageAbove {
    note: Value,
    build: Value,
}

/// The package root above a snapshot whose root holds no package manifest
/// while a directory above it, inside its workspace, does. Entrypoints came
/// from file conventions only, and the mention scan read only the
/// snapshot, so the dead-code findings ([`SCOPE_JUDGED`]) miss the
/// manifest's entries (`main`, `exports`, `bin`, crate targets) and every
/// reference outside the snapshot (tests, scripts, docs): none claims more
/// than tier 60, and the build of the package root is the runnable lead.
fn package_above(
    root: &Path,
    workspace: Option<&str>,
    no_components: bool,
    detectors: &[String],
    scan: &Value,
) -> Option<PackageAbove> {
    let judged = SCOPE_JUDGED
        .iter()
        .filter(|judged| detectors.is_empty() || detectors.iter().any(|d| d == *judged))
        .copied()
        .collect::<Vec<_>>();
    if judged.is_empty() || !no_components {
        return None;
    }
    let workspace = Path::new(workspace?);
    let (dir, manifest) = root
        .ancestors()
        .skip(1)
        .take_while(|dir| dir.starts_with(workspace))
        .find_map(|dir| {
            super::super::workspace::MANIFEST_FILES
                .iter()
                .find(|file| dir.join(file).is_file())
                .map(|file| (dir, *file))
        })?;
    let shown = dir
        .strip_prefix(workspace)
        .ok()
        .map(|relative| relative.to_string_lossy().into_owned())
        .filter(|relative| !relative.is_empty())
        .unwrap_or_else(|| ".".into());
    // The wider build keeps the snapshot's scope: its walk, `exclude` and
    // `parserGlobs`, rebased onto the directory above.
    let mut row = json!({"path": shown});
    BuildScope::of_manifest(scan)
        .above(root.strip_prefix(dir).ok()?)
        .write_row(&mut row);
    Some(PackageAbove {
        note: json!({
            "detectors": judged,
            "skipped": format!(
                "the package root {shown} (it holds {manifest}) lies above the snapshot: its entries and every reference outside the snapshot were not read, so these findings are at most tier 60; the graphBuild in related builds {shown}"
            ),
        }),
        build: crate::response::result::Continuation::related(crate::id::ToolId::GraphBuild, row)
            .build(),
    })
}

/// Row fields that are each finding's own whatever its detector: its
/// identity, its rank and the detectors that corroborate it.
const FINDING_OWN: [&str; 6] = [
    "id",
    "detector",
    "subject",
    "score",
    "impact",
    "corroboratedBy",
];

/// What every listed row of a detector states alike, by detector, taken off
/// its rows: a fact of the detector is stated once (a `summary.notes` entry
/// `{detector, shared}`), not on each of its rows. A field moves
/// when two or more rows list the detector and all state one value (an
/// evidence key alike, under `evidence`); `controls` keeps the ones every
/// row applied, and a row keeps its own beside them. A field that differs,
/// a detector of one row, and each row's own fields ([`FINDING_OWN`]) stay
/// on the rows.
fn state_detector_constants(rows: &mut [Value]) -> Map<String, Value> {
    let mut by_detector = BTreeMap::<String, Vec<usize>>::new();
    for (index, row) in rows.iter().enumerate() {
        if let Some(detector) = row["detector"].as_str() {
            by_detector
                .entry(detector.to_owned())
                .or_default()
                .push(index);
        }
    }
    let mut stated = Map::new();
    for (detector, listed) in by_detector {
        let Some((first, others)) = listed
            .split_first()
            .filter(|(_, others)| !others.is_empty())
        else {
            continue;
        };
        let shared = |value: &Value, at: &dyn Fn(&Value) -> Option<&Value>| {
            others.iter().all(|other| at(&rows[*other]) == Some(value))
        };
        let mut constants = Map::new();
        for (key, value) in rows[*first].as_object().into_iter().flatten() {
            if FINDING_OWN.contains(&key.as_str())
                || matches!(key.as_str(), "controls" | "evidence")
            {
                continue;
            }
            if shared(value, &|row| row.get(key)) {
                constants.insert(key.clone(), value.clone());
            }
        }
        let mut evidence = Map::new();
        for (key, value) in rows[*first]["evidence"].as_object().into_iter().flatten() {
            if shared(value, &|row| row["evidence"].get(key)) {
                evidence.insert(key.clone(), value.clone());
            }
        }
        let controls = rows[*first]["controls"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|control| {
                others.iter().all(|other| {
                    rows[*other]["controls"]
                        .as_array()
                        .is_some_and(|own| own.contains(control))
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        for index in &listed {
            let Some(row) = rows[*index].as_object_mut() else {
                continue;
            };
            for key in constants.keys() {
                row.shift_remove(key);
            }
            if let Some(Value::Object(own)) = row.get_mut("evidence") {
                own.retain(|key, _| !evidence.contains_key(key));
                if own.is_empty() {
                    row.shift_remove("evidence");
                }
            }
            if let Some(Value::Array(own)) = row.get_mut("controls") {
                own.retain(|control| !controls.contains(control));
                if own.is_empty() {
                    row.shift_remove("controls");
                }
            }
        }
        if !controls.is_empty() {
            constants.insert("controls".into(), Value::Array(controls));
        }
        if !evidence.is_empty() {
            constants.insert("evidence".into(), Value::Object(evidence));
        }
        if !constants.is_empty() {
            stated.insert(detector, Value::Object(constants));
        }
    }
    stated
}

#[cfg(test)]
mod tests;

/// The entries whose re-exports made files public API, named by an
/// `evaluated` note of the export detectors, as one `deps` call each: the
/// files those entries re-export, whose exports were not judged.
fn reexport_leads(summary: &mut Value, options: &QueryOptions, graph: &str) -> Vec<Value> {
    let mut leads = Vec::new();
    for note in summary["notes"].as_array_mut().into_iter().flatten() {
        let Some(entries) = note
            .get_mut("evaluated")
            .and_then(Value::as_object_mut)
            .and_then(|evaluated| evaluated.remove("reexportingEntries"))
        else {
            continue;
        };
        for entry in entries.as_array().into_iter().flatten() {
            let mut row = Map::new();
            row.insert("operation".into(), json!("deps"));
            super::paging::pin(&mut row, options, graph);
            row.insert("target".into(), entry.clone());
            leads.push(
                crate::response::result::Continuation::related(
                    crate::id::ToolId::GraphQuery,
                    Value::Object(row),
                )
                .build(),
            );
        }
    }
    leads
}
