//! Change assessment: impact (optionally since a git revision), issues,
//! drift and baselines.
use super::*;

mod changed;
mod manifest_diff;

pub(super) use changed::git;
use changed::git_changed;

/// An impact answer: the first page's facts, the affected-file rows, and
/// the summary's lists by name.
type ImpactAnswer = (
    Map<String, Value>,
    Vec<Value>,
    BTreeMap<&'static str, Vec<Value>>,
);

impl Query<'_> {
    pub(super) fn impact(&self) -> Result<GraphOutput, GraphOutput> {
        unknown_list(self.options)?;
        let (extra, page) = self.complete_rows(|| self.impact_rows())?;
        Ok(run_page(self.options, self.g, extra, page))
    }

    /// The blast radius's complete rows and the facts its first page states;
    /// with `list`, the complete rows of that summary list instead.
    fn impact_rows(&self) -> Result<(Map<String, Value>, Vec<Value>), GraphOutput> {
        let (extra, rows, mut lists) = self.impact_answer()?;
        let Some(list) = self.options.list.as_deref() else {
            return Ok((extra, rows));
        };
        let mut extra = Map::new();
        extra.insert("list".into(), json!(list));
        Ok((extra, lists.remove(list).unwrap_or_default()))
    }

    /// The blast radius: the first page's facts, the affected-file rows and
    /// the summary's lists by name ([`super::super::impact::LISTS`]), which
    /// the summary counts.
    fn impact_answer(&self) -> Result<ImpactAnswer, GraphOutput> {
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
        let mut config_changes = Vec::new();
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
                    let scope = crate::ast_graph::paths::parent(&path).to_owned();
                    let diff = manifest_diff::manifest_diff(&root, rev, &path, change, self.cancel);
                    // A change confined to keys no linking step reads
                    // changes no edge, so it seeds nothing.
                    let seeded = diff.as_ref().is_none_or(|diff| diff.read);
                    let mut entry = json!({
                        "path": path,
                        "scope": if scope.is_empty() { "." } else { scope.as_str() },
                        "seeded": seeded,
                    });
                    if let Some(diff) = diff {
                        entry["keys"] = json!(diff.keys);
                    }
                    config_changes.push(entry);
                    if seeded {
                        global.push((scope, path));
                    }
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
                "reason": if !config_changes.is_empty() {
                    format!(
                        "no graph file changed since {rev}, and the changed manifests changed only keys no linking step reads"
                    )
                } else if unmapped.is_empty() {
                    format!("no files changed since {rev}")
                } else {
                    format!("no file changed since {rev} is in the graph")
                },
            });
            if !unmapped.is_empty() {
                summary["unmapped"] = json!(unmapped.len());
            }
            if !config_changes.is_empty() {
                summary["configChanges"] = json!(config_changes);
            }
            let mut extra = Map::new();
            extra.insert("summary".into(), summary);
            let lists = BTreeMap::from([("unmapped", unmapped)]);
            return Ok((extra, Vec::new(), lists));
        }
        let mut seeds = Vec::new();
        // A directory that names no node changes every file under it, as
        // `deps` and `dependents` read one.
        let files_only = self.flags.kind.is_none_or(|kind| kind == NodeKind::File);
        for reference in &refs {
            match g.directory(reference).filter(|_| files_only) {
                Some((_, files)) => seeds.extend(files),
                None => seeds.push(self.resolve(reference, self.flags.kind)?),
            }
        }
        seeds.sort_unstable();
        seeds.dedup();
        let max_depth = self.flags.depth.unwrap_or(DEFAULT_IMPACT_DEPTH);
        let impact = super::super::impact::run(&g.t, &seeds, &global, max_depth);
        let mut extra = Map::new();
        let mut summary = impact.summary;
        summary["maxDepth"] = json!(max_depth);
        if !config_changes.is_empty() {
            summary["configChanges"] = json!(config_changes);
        }
        if let Some(first) = seeds.first() {
            extra.insert("coverage".into(), g.coverage(*first));
        }
        if !unmapped.is_empty() {
            summary["unmapped"] = json!(unmapped.len());
        }
        extra.insert("summary".into(), summary);
        let lists = BTreeMap::from([
            ("affectedEntrypoints", impact.lists.entrypoints),
            ("testsToRun", impact.lists.tests),
            ("unmapped", unmapped),
        ]);
        // A changed symbol's reach is syntax: lspSearch checks its references.
        let mut related = Vec::new();
        for seed in &seeds {
            related.extend(g.lsp_check(*seed, "references", self.cancel)?);
        }
        if let Some(unlisted) = impact.unlisted {
            extra.insert(
                "warnings".into(),
                json!([format!(
                    "maxDepth {max_depth} left {} unlisted; the related graphQuery call with depth {} lists every file the change reaches.",
                    octocode_engine::text::counted(
                        unlisted.files,
                        "affected file",
                        "affected files"
                    ),
                    unlisted.depth
                )]),
            );
            let mut call = super::paging::query_row(options, &g.id);
            call.remove("list");
            call.insert("depth".into(), json!(unlisted.depth));
            related.insert(
                0,
                octocode_runtime_base::response::result::Continuation::related(
                    octocode_runtime_base::id::ToolId::GraphQuery,
                    Value::Object(call),
                )
                .build(),
            );
        }
        if !related.is_empty() {
            extra.insert("related".into(), Value::Array(related));
        }
        Ok((extra, impact.rows, lists))
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
        let report = super::super::detect::kept_run(
            &g.id,
            &g.t,
            &detectors,
            scan_root,
            entries,
            paths.global_ignore(),
            &stopped,
        );
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
        let mut hidden_lead = None;
        let mut warnings = Vec::new();
        if hidden > 0
            && let Some(notes) = summary["notes"].as_array_mut()
        {
            // byDetector counts before this filter, so its sum is not the
            // page's pagination.total: say by how much and why.
            let filters = options.min_score.map_or_else(
                || format!("minTier {min_tier}"),
                |score| format!("minTier {min_tier} and minScore {score}"),
            );
            // The tier that lists every finding only `minTier` hides.
            let reveal = report
                .findings
                .iter()
                .filter(|f| options.min_score.is_none_or(|min| f.score >= min))
                .map(tier)
                .filter(|found| *found < min_tier)
                .min();
            // The call that lists them is named by what tells it apart, on
            // every page that lists findings or none: a warning that names
            // `related` keeps it beside the rows.
            if let Some(lowest) = reveal {
                warnings.push(format!(
                    "{filters} hid {hidden} findings; the related graphQuery call with minTier {lowest} lists them."
                ));
            }
            hidden_lead = reveal.map(|lowest| {
                let mut call = Map::new();
                super::paging::pin(&mut call, options, &g.id);
                call.insert("operation".into(), json!("issues"));
                if !options.detectors.is_empty() {
                    call.insert("detectors".into(), json!(options.detectors));
                }
                if !options.entries.is_empty() {
                    call.insert("entries".into(), json!(options.entries));
                }
                if let Some(score) = options.min_score {
                    call.insert("minScore".into(), json!(score));
                }
                call.insert("minTier".into(), json!(lowest));
                octocode_runtime_base::response::result::Continuation::related(
                    octocode_runtime_base::id::ToolId::GraphQuery,
                    Value::Object(call),
                )
                .build()
            });
            let rest = if reveal.is_some() {
                ""
            } else {
                " Lower them to list the rest."
            };
            notes.push(json!({
                "filter": "minTier",
                "explanation": format!(
                    "{filters} hid {hidden} findings (hiddenBelowTier): byDetector counts them, so its sum is pagination.total plus hiddenBelowTier.{rest}"
                ),
            }));
        }
        let mut rows = findings
            .iter()
            .map(super::super::detect::Finding::to_json)
            .collect::<Vec<_>>();
        let constants = state_detector_constants(&mut rows);
        let (cycle_lead, cycle_warning) = self.cap_cycle_members(&mut rows);
        warnings.extend(cycle_warning);
        if !warnings.is_empty() {
            extra.insert("warnings".into(), json!(warnings));
        }
        if let Some(notes) = summary["notes"].as_array_mut() {
            notes.extend(
                constants
                    .into_iter()
                    .map(|(detector, shared)| json!({"detector": detector, "shared": shared})),
            );
        }
        let mut leads = hidden_lead.into_iter().collect::<Vec<_>>();
        leads.extend(above.map(|above| above.build));
        leads.extend(cycle_lead);
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

    /// Cuts each cycle finding's `members` to the preview the `cycles`
    /// operation shows, so a large cycle states its `size` and the first
    /// members; the `cycles` read (the returned lead) lists the same cycles
    /// and pages every member of each past its preview.
    fn cap_cycle_members(&self, rows: &mut [Value]) -> (Option<Value>, Option<String>) {
        let mut cut = 0_usize;
        for row in rows.iter_mut().filter(|row| row["detector"] == "cycle") {
            let Some(members) = row["evidence"]["members"].as_array() else {
                continue;
            };
            if members.len() > super::MAX_CYCLE_MEMBERS {
                let preview = members
                    .iter()
                    .take(super::MAX_CYCLE_MEMBERS)
                    .cloned()
                    .collect::<Vec<_>>();
                row["evidence"]["members"] = Value::Array(preview);
                row["evidence"]["membersTruncated"] = json!(true);
                cut += 1;
            }
        }
        let warning = (cut > 0).then(|| {
            // The `cycles` read counts every file of a loop; a finding keeps
            // production files only, so its size can be smaller: say so.
            format!(
                "{cut} cycle(s) list only the first {} members (membersTruncated); related lists every member of each, and counts non-production files (tests, config) that a finding leaves out.",
                super::MAX_CYCLE_MEMBERS
            )
        });
        let lead = (cut > 0).then(|| {
            let mut call = Map::new();
            super::paging::pin(&mut call, self.options, &self.g.id);
            call.insert("operation".into(), json!("cycles"));
            octocode_runtime_base::response::result::Continuation::related(
                octocode_runtime_base::id::ToolId::GraphQuery,
                Value::Object(call),
            )
            .build()
        });
        (lead, warning)
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
            paths.global_ignore(),
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
    let found = super::package_above::package_root(root, workspace, scan)?;
    let (named, manifest) = (found.named(), found.manifest);
    Some(PackageAbove {
        note: json!({
            "detectors": judged,
            "skipped": format!(
                "the package root, {named} (it holds {manifest}), lies above the snapshot: its entries and every reference outside the snapshot were not read, so these findings are at most tier 60; the graphBuild in related builds that root"
            ),
        }),
        build: found.build,
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
                octocode_runtime_base::response::result::Continuation::related(
                    octocode_runtime_base::id::ToolId::GraphQuery,
                    Value::Object(row),
                )
                .build(),
            );
        }
    }
    leads
}
