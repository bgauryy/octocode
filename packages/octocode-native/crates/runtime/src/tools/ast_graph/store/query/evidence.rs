//! Bounded pages of source occurrences, independent of topology deduplication.
use super::*;

impl Query<'_> {
    fn evidence_scope(&self, reference: Option<&str>) -> Result<Option<u32>, GraphOutput> {
        reference
            .map(|reference| self.resolve(reference, None))
            .transpose()
    }

    /// Whether `node` is, or sits in a file among, `scopes`; no scopes admit all.
    fn within_evidence_scope(&self, node: u32, scopes: &[u32]) -> bool {
        scopes.is_empty()
            || scopes.iter().any(|&scope| {
                node == scope
                    || (self.g.t.nodes[scope as usize].kind == NodeKind::File
                        && self.g.t.nodes[node as usize].file == scope)
            })
    }

    /// Whether unresolved reason string `id` passes `reasons`, memoized per id.
    fn admits_reason(
        &self,
        id: u32,
        seen: &mut std::collections::HashMap<u32, bool>,
    ) -> Result<bool, GraphOutput> {
        if self.options.reasons.is_empty() {
            return Ok(true);
        }
        if let Some(&admitted) = seen.get(&id) {
            return Ok(admitted);
        }
        let reason = self.g.text(id, self.cancel)?;
        let admitted = self.options.reasons.iter().any(|wanted| *wanted == reason);
        seen.insert(id, admitted);
        Ok(admitted)
    }

    pub(super) fn evidence(&self) -> Result<GraphOutput, GraphOutput> {
        let mut sources = Vec::new();
        for reference in self.options.sources() {
            sources.extend(self.evidence_scope(Some(reference))?);
        }
        let source = sources.as_slice();
        let destination = self.evidence_scope(self.options.to.as_deref())?;
        let destination = destination.as_slice();
        let mut reasons = std::collections::HashMap::new();
        let mut window = Window::budgeted(self.window(), self.g.page_budget);
        let mut keep = |row: &dyn Fn() -> Result<Value, GraphOutput>| -> Result<(), GraphOutput> {
            window.try_push(row)
        };
        if self.options.op == "witnesses" {
            let filter = self.filter(&EdgeKind::ALL);
            for witness in &self.g.t.witnesses {
                self.cancel
                    .check()
                    .map_err(|message| GraphOutput::error(ErrorCode::Cancelled, message))?;
                let edge = &witness.edge;
                if !self.within_evidence_scope(edge.src, source)
                    || !self.within_evidence_scope(edge.dst, destination)
                    || !filter.admits(edge)
                {
                    continue;
                }
                keep(&|| {
                    let text = |id| self.g.text(id, self.cancel);
                    let source_digest = text(witness.provenance.source_digest)?;
                    let file = self.g.key(self.g.t.nodes[edge.src as usize].file);
                    let version = text(witness.provenance.derivation_version)?;
                    let detail = text(edge.detail)?;
                    let range = source_range(witness.provenance)?;
                    let origin = self.g.project_origin(
                        witness.provenance.source_origin,
                        file,
                        &source_digest,
                        self.cancel,
                    )?;
                    let rule = derivation_rule(edge.kind, &detail);
                    let identity = json!([
                        source_digest,
                        text(witness.occurrence)?,
                        self.g.key(edge.src),
                        self.g.key(edge.dst),
                        &rule,
                        version,
                        &range,
                        origin.analysis_unit_id,
                    ])
                    .to_string();
                    let mut row = json!({
                        "id": octocode_engine::digest::sha256(identity.as_bytes()),
                        "from": self.g.key(edge.src), "to": self.g.key(edge.dst),
                        "edge": edge.kind.as_str(), "detail": detail,
                        "file": file,
                        "line": (edge.line != NONE).then_some(edge.line), "confidence": edge.confidence.as_str(), "test": edge.test,
                        "sourceDigest": source_digest, "evidence": "syntax",
                        "sourceRange": range, "sourceOrigin": origin.value,
                    });
                    self.derivation(&mut row, rule, &version);
                    argument_fields(self.g, witness.argument, &mut row, self.cancel)?;
                    Ok(row)
                })?;
            }
        } else {
            let filter = self.filter(&[EdgeKind::Calls, EdgeKind::Passes]);
            for site in &self.g.t.unresolved {
                self.cancel
                    .check()
                    .map_err(|message| GraphOutput::error(ErrorCode::Cancelled, message))?;
                if !self.within_evidence_scope(site.src, source)
                    || !filter.kinds.contains(&site.relation)
                    || !self.admits_reason(site.reason, &mut reasons)?
                {
                    continue;
                }
                keep(&|| {
                    let text = |id| self.g.text(id, self.cancel);
                    let source_digest = text(site.provenance.source_digest)?;
                    let file = self.g.key(self.g.t.nodes[site.src as usize].file);
                    let version = text(site.provenance.derivation_version)?;
                    let reason = text(site.reason)?;
                    let callee = text(site.callee)?;
                    let range = source_range(site.provenance)?;
                    let origin = self.g.project_origin(
                        site.provenance.source_origin,
                        file,
                        &source_digest,
                        self.cancel,
                    )?;
                    let rule = derivation_rule(site.relation, &format!("unresolved:{reason}"));
                    let identity = json!([
                        source_digest,
                        self.g.key(site.src),
                        text(site.occurrence)?,
                        callee,
                        &rule,
                        version,
                        &range,
                        origin.analysis_unit_id,
                    ])
                    .to_string();
                    let mut row = json!({
                        "id": octocode_engine::digest::sha256(identity.as_bytes()),
                        "from": self.g.key(site.src), "targetName": callee,
                        "edge": site.relation.as_str(),
                        "file": file,
                        "line": site.line, "reason": reason,
                        "sourceDigest": source_digest, "evidence": "syntax",
                        "sourceRange": range, "sourceOrigin": origin.value,
                    });
                    self.derivation(&mut row, rule, &version);
                    argument_fields(self.g, site.argument, &mut row, self.cancel)?;
                    Ok(row)
                })?;
            }
        }
        let mut extra = Map::new();
        extra.insert("evidence".into(), json!("syntax"));
        // One source scopes coverage to its language; several, or none, the snapshot.
        let scope = match source {
            [only] => Some(*only),
            _ => None,
        };
        extra.insert("coverage".into(), self.g.evidence_coverage(scope));
        extra.insert(
            "argumentCoverage".into(),
            self.g.manifest["argumentCoverage"].clone(),
        );
        Ok(run_page(self.options, self.g, extra, window.finish()))
    }
}

impl Query<'_> {
    /// `derivation` on full rows; lean rows omit it (`edge` and `detail` or
    /// `reason` name the rule; `includeProvenance` returns it).
    fn derivation(&self, row: &mut Value, rule: String, version: &str) {
        if !self.g.compact_provenance {
            row["derivation"] = json!({"rule": rule, "version": version});
        }
    }
}

fn source_range(provenance: ProvenanceRec) -> Result<Value, GraphOutput> {
    if provenance.source_start == NONE {
        Ok(Value::Null)
    } else {
        sources::range(octocode_engine::text::analysis::ByteRange::new(
            provenance.source_start,
            provenance.source_end,
        ))
    }
}

fn derivation_rule(edge: EdgeKind, detail: &str) -> String {
    if edge == EdgeKind::Passes {
        format!("syntax-argument-reference:1:{detail}")
    } else {
        format!("{}:{detail}", edge.as_str())
    }
}

fn argument_fields(
    graph: &Graph,
    reference: u32,
    row: &mut Value,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<(), GraphOutput> {
    use super::super::format::arguments::{ArgumentEvidence, Observation};
    if reference == NONE {
        return Ok(());
    }
    let evidence: ArgumentEvidence = serde_json::from_str(&graph.text(reference, cancel)?)
        .map_err(|error| {
            GraphOutput::error(
                ErrorCode::InvalidInput,
                format!("invalid stored argument evidence: {error}"),
            )
        })?;
    let range = sources::range;
    let file = row["file"].as_str().unwrap_or_default().to_owned();
    let digest = row["sourceDigest"].as_str().unwrap_or_default().to_owned();
    let origin = |reference| {
        graph
            .project_origin(reference, &file, &digest, cancel)
            .map(|origin| origin.value)
    };
    row["recipient"] = json!({
        "occurrenceId":evidence.recipient,
        "sourceRange":range(evidence.recipient_range.range)?,
        "sourceOrigin":origin(evidence.recipient_range.origin)?,
    });
    row["parseAssessment"] = json!(evidence.assessment);
    let mut argument = json!({"groupIndex":evidence.group});
    match evidence.observation {
        Observation::Reference {
            index,
            label,
            target,
            form,
            selection,
            blocker,
        } => {
            argument["index"] = json!(index);
            argument["form"] = json!(form);
            if let Some(label) = label {
                argument["label"] = json!(label);
            }
            row["targetName"] = json!(target);
            row["selectionByteRange"] = json!(range(selection.range)?);
            row["selectionOrigin"] = origin(selection.origin)?;
            if let Some(blocker) = blocker {
                row["bindingSourceRange"] = json!(range(blocker.range)?);
                row["bindingOrigin"] = origin(blocker.origin)?;
            }
        }
        Observation::UnsupportedSlot {
            index,
            label,
            construct,
        } => {
            argument["index"] = json!(index);
            argument["form"] = json!("unsupported");
            if let Some(label) = label {
                argument["label"] = json!(label);
            }
            row["construct"] = json!(construct);
            if let Some(object) = row.as_object_mut() {
                object.remove("targetName");
            }
        }
        Observation::UnsupportedGroup { construct } => {
            argument["form"] = json!("unsupportedGroup");
            row["construct"] = json!(construct);
            if let Some(object) = row.as_object_mut() {
                object.remove("targetName");
            }
        }
    }
    row["argument"] = argument;
    Ok(())
}
