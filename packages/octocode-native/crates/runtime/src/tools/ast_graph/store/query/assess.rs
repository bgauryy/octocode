//! Change assessment: impact (optionally since a git revision), issues,
//! drift and baselines.
use super::*;
use crate::tools::ast_graph::build_scope::BuildScope;

mod changed;

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
            for path in changed {
                if super::super::super::freshness::is_global_config(&path) {
                    global.push((
                        crate::tools::ast_graph::paths::parent(&path).to_owned(),
                        path,
                    ));
                } else if g.t.by_key(&path).is_some() {
                    refs.push(path);
                } else {
                    unmapped.push(path);
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
        let keep = |f: &&super::super::detect::Finding| {
            options.min_score.is_none_or(|min| f.score >= min) && f.tier >= min_tier
        };
        let total_before = report.findings.len();
        let findings = report
            .findings
            .iter()
            .filter(keep)
            .cloned()
            .collect::<Vec<_>>();
        let mut extra = Map::new();
        let mut summary = report.summary.clone();
        if let Some(note) = manifest_above(
            &root,
            g.manifest["workspace"].as_str(),
            g.t.components.is_empty(),
            &detectors,
            &g.manifest["scan"],
        ) && let Some(notes) = summary["notes"].as_array_mut()
        {
            notes.push(note);
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
                "hidden": hidden,
                "explanation": format!(
                    "byDetector counts all {total_before} findings the detectors raised; {filters} hid {hidden}, so {} are listed (pagination.total). Lower them to list the rest.",
                    findings.len()
                ),
            }));
        }
        let shared = shared_controls(&findings);
        if let Some(notes) = summary["notes"].as_array_mut() {
            notes.extend(
                shared.iter().map(
                    |(detector, controls)| json!({"detector": detector, "controls": controls}),
                ),
            );
        }
        let leads = reexport_leads(&mut summary, options, &g.id);
        if !leads.is_empty() {
            extra.insert("related".into(), Value::Array(leads));
        }
        extra.insert("summary".into(), summary);
        let mut status = BTreeMap::<String, &str>::new();
        if let Some(base) = &options.baseline {
            let baseline =
                self.baseline(paths, base, &detectors, entries, &findings, &mut status)?;
            extra.insert("baseline".into(), baseline);
        }
        let rows = findings
            .iter()
            .map(|finding| {
                let mut row = finding.to_json();
                own_controls(&mut row, finding, &shared);
                if let Some(state) = status.get(&finding.id) {
                    row["status"] = json!(state);
                }
                row
            })
            .collect::<Vec<_>>();
        Ok((extra, rows))
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
    /// or `existing` (written to `status`), and the resolved ones are listed.
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

/// The note of a snapshot whose root holds no package manifest while a
/// directory above it, inside its workspace, does: entrypoints came from
/// file conventions only, so the reachability findings miss the manifest's
/// entries (`main`, `exports`, `bin`, crate targets).
fn manifest_above(
    root: &Path,
    workspace: Option<&str>,
    no_components: bool,
    detectors: &[String],
    scan: &Value,
) -> Option<Value> {
    let judged = detectors.is_empty()
        || detectors.iter().any(|detector| {
            matches!(
                detector.as_str(),
                "unreachable-file" | "unused-export" | "test-only"
            )
        });
    if !judged || !no_components {
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
    let Value::Object(row) = row else {
        return None;
    };
    Some(json!({
        "detector": "unreachable-file",
        "skipped": format!(
            "the snapshot root holds no package manifest, so entrypoints came from file conventions only; {shown} holds {manifest}: build it with {} so its entries count",
            super::paging::call(crate::id::ToolId::GraphBuild.as_str(), row)
        ),
    }))
}

/// The false-positive controls every listed finding of a detector applied,
/// by detector: a fact of the detector, stated once (a `summary.notes`
/// entry `{detector, controls}`) instead of on each of its rows.
fn shared_controls(
    findings: &[super::super::detect::Finding],
) -> BTreeMap<&'static str, Vec<&'static str>> {
    let mut shared = BTreeMap::<&'static str, Vec<&'static str>>::new();
    for finding in findings {
        shared
            .entry(finding.detector)
            .and_modify(|common| common.retain(|control| finding.controls.contains(control)))
            .or_insert_with(|| finding.controls.clone());
    }
    shared.retain(|_, common| !common.is_empty());
    shared
}

/// Leaves on `row` only the controls its finding applied beyond its
/// detector's [`shared_controls`].
fn own_controls(
    row: &mut Value,
    finding: &super::super::detect::Finding,
    shared: &BTreeMap<&'static str, Vec<&'static str>>,
) {
    let Some(common) = shared.get(finding.detector) else {
        return;
    };
    let own = finding
        .controls
        .iter()
        .filter(|control| !common.contains(control))
        .collect::<Vec<_>>();
    if let Some(fields) = row.as_object_mut() {
        if own.is_empty() {
            fields.shift_remove("controls");
        } else {
            fields.insert("controls".into(), json!(own));
        }
    }
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
