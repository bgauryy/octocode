//! Bounded pages of source occurrences, independent of topology deduplication.
use super::*;

impl Query<'_> {
    fn evidence_scope(&self, reference: Option<&str>) -> Result<Option<u32>, GraphOutput> {
        reference
            .map(|reference| self.resolve(reference, None))
            .transpose()
    }

    fn within_evidence_scope(&self, node: u32, scope: Option<u32>) -> bool {
        scope.is_none_or(|scope| {
            node == scope
                || (self.g.t.nodes[scope as usize].kind == NodeKind::File
                    && self.g.t.nodes[node as usize].file == scope)
        })
    }

    pub(super) fn evidence(&self) -> Result<GraphOutput, GraphOutput> {
        let source = self.evidence_scope(self.options.target.as_deref())?;
        let destination = self.evidence_scope(self.options.to.as_deref())?;
        let mut window = Window::new(self.window());
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
                    let source_digest = self.g.t.str(witness.provenance.source_digest);
                    let version = self.g.t.str(witness.provenance.derivation_version);
                    let range = source_range(witness.provenance)?;
                    let origin = self
                        .g
                        .origin(witness.provenance.source_origin, self.cancel)?;
                    let rule = derivation_rule(edge.kind, self.g.t.str(edge.detail));
                    let identity = json!([
                        source_digest,
                        self.g.t.str(witness.occurrence),
                        self.g.key(edge.src),
                        self.g.key(edge.dst),
                        &rule,
                        version,
                        &range,
                        origin.get("analysisUnitId"),
                    ])
                    .to_string();
                    let mut row = json!({
                        "id": octocode_engine::digest::sha256(identity.as_bytes()),
                        "from": self.g.key(edge.src), "to": self.g.key(edge.dst),
                        "edge": edge.kind.as_str(), "detail": self.g.t.str(edge.detail),
                        "file": self.g.key(self.g.t.nodes[edge.src as usize].file),
                        "line": (edge.line != NONE).then_some(edge.line), "confidence": edge.confidence.as_str(), "test": edge.test,
                        "sourceDigest": source_digest, "evidence": "syntax",
                        "sourceRange": range, "sourceOrigin": origin, "derivation": {"rule":rule,"version":version},
                    });
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
                {
                    continue;
                }
                keep(&|| {
                    let source_digest = self.g.t.str(site.provenance.source_digest);
                    let version = self.g.t.str(site.provenance.derivation_version);
                    let range = source_range(site.provenance)?;
                    let origin = self.g.origin(site.provenance.source_origin, self.cancel)?;
                    let rule = derivation_rule(
                        site.relation,
                        &format!("unresolved:{}", self.g.t.str(site.reason)),
                    );
                    let identity = json!([
                        source_digest,
                        self.g.key(site.src),
                        self.g.t.str(site.occurrence),
                        self.g.t.str(site.callee),
                        &rule,
                        version,
                        &range,
                        origin.get("analysisUnitId"),
                    ])
                    .to_string();
                    let mut row = json!({
                        "id": octocode_engine::digest::sha256(identity.as_bytes()),
                        "from": self.g.key(site.src), "targetName": self.g.t.str(site.callee),
                        "edge": site.relation.as_str(),
                        "file": self.g.key(self.g.t.nodes[site.src as usize].file),
                        "line": site.line, "reason": self.g.t.str(site.reason),
                        "sourceDigest": source_digest, "evidence": "syntax",
                        "sourceRange": range, "sourceOrigin": origin, "derivation": {"rule":rule,"version":version},
                    });
                    argument_fields(self.g, site.argument, &mut row, self.cancel)?;
                    Ok(row)
                })?;
            }
        }
        let mut extra = Map::new();
        extra.insert("evidence".into(), json!("syntax"));
        extra.insert("coverage".into(), self.g.evidence_coverage(source));
        extra.insert(
            "argumentCoverage".into(),
            self.g.manifest["argumentCoverage"].clone(),
        );
        Ok(run_page(self.options, self.g, extra, window.finish()))
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
    let evidence: ArgumentEvidence =
        serde_json::from_str(graph.t.str(reference)).map_err(|error| {
            GraphOutput::error(
                ErrorCode::InvalidInput,
                format!("invalid stored argument evidence: {error}"),
            )
        })?;
    let range = sources::range;
    row["recipient"] = json!({
        "occurrenceId":evidence.recipient,
        "sourceRange":range(evidence.recipient_range.range)?,
        "sourceOrigin":graph.origin(evidence.recipient_range.origin, cancel)?,
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
            row["selectionOrigin"] = graph.origin(selection.origin, cancel)?;
            if let Some(blocker) = blocker {
                row["bindingSourceRange"] = json!(range(blocker.range)?);
                row["bindingOrigin"] = graph.origin(blocker.origin, cancel)?;
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
