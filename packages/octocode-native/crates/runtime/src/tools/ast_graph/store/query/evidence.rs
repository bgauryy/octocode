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

    /// Whether target-name string `id` passes `name`: the written name, or
    /// its last segment (`f` of `a.f`, `A::f`), memoized per id.
    fn admits_name(
        &self,
        id: u32,
        seen: &mut std::collections::HashMap<u32, bool>,
    ) -> Result<bool, GraphOutput> {
        let Some(wanted) = self.options.symbol_name.as_deref() else {
            return Ok(true);
        };
        if let Some(&admitted) = seen.get(&id) {
            return Ok(admitted);
        }
        let callee = self.g.text(id, self.cancel)?;
        let admitted = callee == wanted || callee.rsplit(['.', ':']).next() == Some(wanted);
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
        let mut names = std::collections::HashMap::new();
        let mut window = Window::budgeted(self.window(), self.g.row_budget());
        let mut keep = |row: &dyn Fn() -> Result<Value, GraphOutput>| -> Result<(), GraphOutput> {
            window.try_push(row)
        };
        // Matching call witnesses the confidence filter leaves out.
        let mut filtered_by_confidence = 0usize;
        if self.options.op == "witnesses" {
            let filter = self.filter(&EdgeKind::ALL);
            for witness in &self.g.t.witnesses {
                self.cancel
                    .check()
                    .map_err(|message| GraphOutput::error(ErrorCode::Cancelled, message))?;
                let edge = &witness.edge;
                if !self.within_evidence_scope(edge.src, source)
                    || !self.within_evidence_scope(edge.dst, destination)
                    || !filter.admits_kind(edge)
                    || !self.admits_name(self.g.t.nodes[edge.dst as usize].name, &mut names)?
                {
                    continue;
                }
                if filter.below_confidence(edge) {
                    filtered_by_confidence += 1;
                    continue;
                }
                keep(&|| self.witness_row(witness))?;
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
                    || !self.admits_name(site.callee, &mut names)?
                {
                    continue;
                }
                keep(&|| self.unresolved_row(site))?;
            }
        }
        let mut extra = Map::new();
        extra.insert("evidence".into(), json!("syntax"));
        if filtered_by_confidence > 0 {
            extra.insert("filteredByConfidence".into(), json!(filtered_by_confidence));
        }
        // One source scopes coverage to its language; several, or none, the snapshot.
        let scope = match source {
            [only] => Some(*only),
            _ => None,
        };
        extra.insert("coverage".into(), self.g.evidence_coverage(scope));
        extra.insert("argumentCoverage".into(), self.g.argument_coverage(source));
        let page = window.finish();
        // `target` is the calling side: a symbol that makes no call listed
        // here may still be the one called, which `to` scopes.
        let mut hints = Vec::new();
        if self.options.op == "witnesses" && page.total == 0 && destination.is_empty() {
            let filter = self.filter(&EdgeKind::ALL);
            let called = self
                .g
                .t
                .witnesses
                .iter()
                .filter(|w| {
                    !source.is_empty()
                        && self.within_evidence_scope(w.edge.dst, source)
                        && filter.admits_kind(&w.edge)
                })
                .count();
            if called > 0 {
                hints.push(format!(
                    "`target` scopes the side that makes the calls, and it makes none here; {called} witnesses name it as the callee: pass it as `to` instead"
                ));
            }
        }
        let mut out = run_page(self.options, self.g, extra, page);
        crate::response::result::add_hints(&mut out.value, hints);
        Ok(out)
    }
}

impl Query<'_> {
    /// A witness row: the edge, its line, and its byte provenance when its
    /// file shows it ([`Graph::shows_provenance`]).
    fn witness_row(
        &self,
        witness: &super::super::format::WitnessRec,
    ) -> Result<Value, GraphOutput> {
        let edge = &witness.edge;
        let text = |id| self.g.text(id, self.cancel);
        let file_node = self.g.t.nodes[edge.src as usize].file;
        let file = self.g.key(file_node);
        let version = text(witness.provenance.derivation_version)?;
        let detail = text(edge.detail)?;
        let rule = derivation_rule(edge.kind, &detail);
        let mut row = json!({
            "from": self.g.key(edge.src), "to": self.g.key(edge.dst),
            "edge": edge.kind.as_str(), "detail": detail,
            "path": file,
            "line": (edge.line != NONE).then_some(edge.line), "confidence": edge.confidence.as_str(),
        });
        let shown = Graph::shows_provenance(&self.g.t.nodes[file_node as usize]);
        let identity = if shown {
            let source_digest = text(witness.provenance.source_digest)?;
            let range = source_range(witness.provenance)?;
            let origin = self.g.project_origin(
                witness.provenance.source_origin,
                file,
                &source_digest,
                self.cancel,
            )?;
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
            row["sourceDigest"] = json!(source_digest);
            row["sourceRange"] = range;
            row["sourceOrigin"] = origin.value;
            Some(identity)
        } else {
            None
        };
        // Absent `test` is false; the page states the evidence.
        if edge.test {
            row["test"] = json!(true);
        }
        if let Some(identity) = identity {
            self.occurrence_id(&mut row, &identity);
        }
        self.derivation(&mut row, rule, &version);
        argument_fields(self.g, witness.argument, &mut row, shown, self.cancel)?;
        Ok(row)
    }

    /// An unresolved site's row, its provenance as [`Self::witness_row`]'s.
    fn unresolved_row(
        &self,
        site: &super::super::format::UnresolvedRec,
    ) -> Result<Value, GraphOutput> {
        let text = |id| self.g.text(id, self.cancel);
        let file_node = self.g.t.nodes[site.src as usize].file;
        let file = self.g.key(file_node);
        let version = text(site.provenance.derivation_version)?;
        let reason = text(site.reason)?;
        let callee = text(site.callee)?;
        let rule = derivation_rule(site.relation, &format!("unresolved:{reason}"));
        let mut row = json!({
            "from": self.g.key(site.src), "targetName": callee,
            "edge": site.relation.as_str(),
            "path": file,
            "line": site.line, "reason": reason,
        });
        let shown = Graph::shows_provenance(&self.g.t.nodes[file_node as usize]);
        if shown {
            let source_digest = text(site.provenance.source_digest)?;
            let range = source_range(site.provenance)?;
            let origin = self.g.project_origin(
                site.provenance.source_origin,
                file,
                &source_digest,
                self.cancel,
            )?;
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
            row["sourceDigest"] = json!(source_digest);
            row["sourceRange"] = range;
            row["sourceOrigin"] = origin.value;
            self.occurrence_id(&mut row, &identity);
        }
        self.derivation(&mut row, rule, &version);
        argument_fields(self.g, site.argument, &mut row, shown, self.cancel)?;
        Ok(row)
    }

    /// The row's occurrence `id`, a digest of its provenance: only under
    /// `includeProvenance:true`, as the digest and spans it comes from.
    fn occurrence_id(&self, row: &mut Value, identity: &str) {
        if !self.g.compact_provenance {
            row["id"] = json!(octocode_engine::digest::sha256(identity.as_bytes()));
        }
    }

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

/// The argument a row observes. `shown`: the row states its byte
/// provenance ([`Graph::shows_provenance`]); otherwise the argument's spans,
/// origins and recipient (whose occurrence id names the source's digest)
/// are left out with the rest of it.
fn argument_fields(
    graph: &Graph,
    reference: u32,
    row: &mut Value,
    shown: bool,
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
    let file = row["path"].as_str().unwrap_or_default().to_owned();
    let digest = row["sourceDigest"].as_str().unwrap_or_default().to_owned();
    let origin = |reference| {
        graph
            .project_origin(reference, &file, &digest, cancel)
            .map(|origin| origin.value)
    };
    if shown {
        row["recipient"] = json!({
            "occurrenceId":evidence.recipient,
            "sourceRange":range(evidence.recipient_range.range)?,
            "sourceOrigin":origin(evidence.recipient_range.origin)?,
        });
    }
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
            if shown {
                row["selectionByteRange"] = json!(range(selection.range)?);
                row["selectionOrigin"] = origin(selection.origin)?;
                if let Some(blocker) = blocker {
                    row["bindingSourceRange"] = json!(range(blocker.range)?);
                    row["bindingOrigin"] = origin(blocker.origin)?;
                }
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
