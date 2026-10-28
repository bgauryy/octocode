//! Change assessment: impact (optionally since a git revision), issues,
//! drift and baselines.
use super::*;

/// Root-relative files changed since `rev` (committed, staged, unstaged) plus
/// untracked files, via git.
pub(super) fn git_changed(root: &Path, rev: &str) -> Result<Vec<String>, String> {
    if rev.starts_with('-') {
        return Err(format!("invalid revision {rev:?}"));
    }
    let run = |args: &[&str]| -> Result<Vec<String>, String> {
        let mut command = std::process::Command::new("git");
        // The repository's own config must not run programs: an fsmonitor
        // hook executes on every index read. No prompts, no index rewrites.
        command
            .arg("-C")
            .arg(root)
            .args(["-c", "core.fsmonitor=false"])
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_OPTIONAL_LOCKS", "0");
        let output = crate::tools::bounded_process::run_bounded(
            command,
            "git",
            std::time::Duration::from_secs(30),
            32 * 1024 * 1024,
            &crate::execution::cancel::NeverCancel,
        )?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect())
    };
    let mut files = run(&["diff", "--name-only", "--relative", rev, "--"])?;
    files.extend(run(&["ls-files", "--others", "--exclude-standard"])?);
    files.sort();
    files.dedup();
    Ok(files)
}

impl Query<'_> {
    pub(super) fn impact(&self) -> Result<GraphOutput, GraphOutput> {
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
            let changed = git_changed(&root, rev)
                .map_err(|message| GraphOutput::error(ErrorCode::GitFailed, message))?;
            for path in changed {
                if super::super::super::freshness::is_global_config(&path) {
                    global.push((
                        crate::tools::ast_graph::graph::parent(&path).to_owned(),
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
                    "`impact` needs a reference, --changed, or --since <rev>",
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
            return Ok(run_page(
                options,
                g,
                extra,
                Page {
                    total: 0,
                    rows: Vec::new(),
                    start: 0,
                },
            ));
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
        let rows = impact.rows;
        Ok(run_page(
            options,
            g,
            extra,
            Page {
                total: rows.len(),
                rows,
                start: 0,
            },
        ))
    }

    pub(super) fn issues(&self, paths: &PathPolicy) -> Result<GraphOutput, GraphOutput> {
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
        let report =
            super::super::detect::run(&g.t, &detectors, scan_root, entries).map_err(invalid)?;
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
        summary["minTier"] = json!(min_tier);
        summary["hiddenBelowTier"] = json!(total_before - findings.len());
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
                if let Some(state) = status.get(&finding.id) {
                    row["status"] = json!(state);
                }
                // Graph verify commands run against this snapshot.
                if let Some(verify) = row.get_mut("verify").and_then(Value::as_array_mut) {
                    for step in verify {
                        if let Some(command) = step
                            .as_str()
                            .filter(|command| command.starts_with("octocode graph query "))
                        {
                            *step = json!(format!("{command} {}", snapshot_flags(options, &g.id)));
                        }
                    }
                }
                row
            })
            .collect::<Vec<_>>();
        Ok(run_page(
            options,
            g,
            extra,
            Page {
                total: rows.len(),
                rows,
                start: 0,
            },
        ))
    }

    /// The structural drift from the `--baseline` snapshot to this one.
    pub(super) fn drift(&self, paths: &PathPolicy) -> Result<GraphOutput, GraphOutput> {
        let base = self.options.baseline.as_deref().ok_or_else(|| {
            GraphOutput::error(
                ErrorCode::InvalidInput,
                "`drift` needs --baseline <snapshot>",
            )
        })?;
        let base_options = QueryOptions {
            graph: Some(base.to_owned()),
            workspace: self.options.workspace.clone(),
            ..Default::default()
        };
        let base_graph = load(&base_options, paths, self.cancel).map_err(|failure| *failure)?;
        let drift = super::super::drift::diff(&base_graph.t, &self.g.t);
        let mut extra = Map::new();
        extra.insert("baseline".into(), base_graph.summary());
        extra.insert("summary".into(), drift.summary);
        let rows = drift.rows;
        Ok(run_page(
            self.options,
            self.g,
            extra,
            Page {
                total: rows.len(),
                rows,
                start: 0,
            },
        ))
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
            ..Default::default()
        };
        let base_graph = load(&base_options, paths, self.cancel).map_err(|failure| *failure)?;
        let base_root = PathBuf::from(base_graph.manifest["root"].as_str().unwrap_or_default());
        let before = super::super::detect::run(
            &base_graph.t,
            detectors,
            base_root.is_dir().then_some(base_root.as_path()),
            entries,
        )
        .map_err(|message| GraphOutput::error(ErrorCode::InvalidInput, message))?;
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
