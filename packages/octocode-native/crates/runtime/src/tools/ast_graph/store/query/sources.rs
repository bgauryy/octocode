use super::super::format::sources::SourceTable;
use super::*;
use crate::execution::cancel::CancellationCheck;
use crate::response::source_evidence;
use octocode_engine::text::analysis::{
    AnalysisMembership, ByteRange, Encoding, OriginBranch, OriginProjection, SourceDescriptors,
};

pub(super) struct ProjectedOrigin {
    pub(super) analysis_unit_id: Option<String>,
    pub(super) value: Value,
}

fn is_identity(
    origin: &OriginProjection,
    descriptors: &SourceDescriptors,
    file: &str,
    digest: &str,
) -> bool {
    let [segment] = origin.segments.as_slice() else {
        return false;
    };
    let [OriginBranch::Copied(raw)] = segment.branches.as_slice() else {
        return false;
    };
    if segment.analysis != AnalysisMembership::Included || raw.raw_range != segment.source_range {
        return false;
    }
    let Some(unit) = descriptors
        .units
        .binary_search_by(|unit| unit.id.cmp(&origin.analysis_unit_id))
        .ok()
        .map(|index| &descriptors.units[index])
    else {
        return false;
    };
    let Some(representation) = descriptors
        .representations
        .binary_search_by(|row| row.id.cmp(&unit.representation_id))
        .ok()
        .map(|index| &descriptors.representations[index])
    else {
        return false;
    };
    let Some(snapshot) = descriptors
        .snapshots
        .binary_search_by(|row| row.id.cmp(&raw.source_id))
        .ok()
        .map(|index| &descriptors.snapshots[index])
    else {
        return false;
    };
    snapshot.path == file
        && snapshot.digest == digest
        && snapshot.encoding == Encoding::Utf8
        && snapshot.bom_bytes == 0
        && representation.source_digest == digest
        && representation.byte_length == snapshot.byte_length
        && representation.source_ids.as_slice() == [raw.source_id.as_str()]
        && unit.regions.as_slice() == [ByteRange::new(0, snapshot.byte_length)]
}

pub(super) fn range(
    range: octocode_engine::text::analysis::ByteRange,
) -> Result<Value, GraphOutput> {
    source_evidence::project_range(range)
        .map(|range| json!(range))
        .map_err(|error| GraphOutput::error(error.code, error.message))
}

impl Graph {
    /// String `id`: a record string from the decoded tables, or an evidence
    /// string read from its authenticated block when the snapshot was opened
    /// for an evidence page. Never a silent empty string for an unread id.
    pub(super) fn text(
        &self,
        id: u32,
        cancel: &dyn CancellationCheck,
    ) -> Result<std::borrow::Cow<'_, str>, GraphOutput> {
        if let Some(text) = self.t.strings.get(id as usize) {
            return Ok(std::borrow::Cow::Borrowed(text));
        }
        let read = self
            .selective
            .as_ref()
            .and_then(|reader| reader.borrow_mut().evidence_string(id, cancel));
        match read {
            Some(Ok(text)) => Ok(std::borrow::Cow::Owned(text)),
            Some(Err(message)) => Err(*super::load::decode_failure(message, cancel)),
            None if id == NONE => Ok(std::borrow::Cow::Borrowed("")),
            None => Err(GraphOutput::error(
                ErrorCode::Decode,
                "graph string reference is outside the decoded dictionary",
            )),
        }
    }

    pub(super) fn project_origin(
        &self,
        reference: u32,
        file: &str,
        digest: &str,
        cancel: &dyn CancellationCheck,
    ) -> Result<ProjectedOrigin, GraphOutput> {
        if !self.compact_provenance || reference == NONE {
            let value = self.origin(reference, cancel)?;
            return Ok(ProjectedOrigin {
                analysis_unit_id: value["analysisUnitId"].as_str().map(str::to_owned),
                value,
            });
        }
        let file = octocode_engine::text::slash_path(
            &Path::new(self.manifest["root"].as_str().unwrap_or_default()).join(file),
        );
        let (origin, identity) = self.with_origin(reference, cancel, |sources| {
            let origin = sources
                .origin(reference, &mut self.source_budget.borrow_mut(), cancel)
                .map_err(|error| GraphOutput::error(error.code, error.message))?;
            let identity = is_identity(&origin, &sources.descriptors, &file, digest);
            Ok((origin, identity))
        })?;
        let value = if identity {
            json!("identity")
        } else {
            json!(
                source_evidence::project_origin(
                    &origin,
                    &mut self.source_budget.borrow_mut(),
                    cancel,
                )
                .map_err(|error| GraphOutput::error(error.code, error.message))?
            )
        };
        Ok(ProjectedOrigin {
            analysis_unit_id: Some(origin.analysis_unit_id),
            value,
        })
    }

    /// Runs `f` on the snapshot's source table; a point reader reads and
    /// authenticates it on first use.
    fn with_sources<R>(
        &self,
        cancel: &dyn CancellationCheck,
        f: impl FnOnce(&SourceTable) -> Result<R, GraphOutput>,
    ) -> Result<R, GraphOutput> {
        self.with_origin(NONE, cancel, f)
    }

    /// [`Self::with_sources`] with origin `reference` (unless `NONE`) loaded:
    /// a reader that reads sources reads only that origin's block.
    fn with_origin<R>(
        &self,
        reference: u32,
        cancel: &dyn CancellationCheck,
        f: impl FnOnce(&SourceTable) -> Result<R, GraphOutput>,
    ) -> Result<R, GraphOutput> {
        if let Some(reader) = &self.selective
            && reader.borrow().reads_sources()
        {
            let mut reader = reader.borrow_mut();
            if reference != NONE {
                reader
                    .load_origin(reference, cancel)
                    .map_err(|message| *decode_failure(message, cancel))?;
            }
            let sources = reader
                .sources(cancel)
                .map_err(|message| *decode_failure(message, cancel))?;
            return f(sources);
        }
        f(&self.t.sources)
    }

    pub(super) fn origin(
        &self,
        reference: u32,
        cancel: &dyn CancellationCheck,
    ) -> Result<Value, GraphOutput> {
        if reference == NONE {
            return Ok(Value::Null);
        }
        let origin = self.with_origin(reference, cancel, |sources| {
            sources
                .origin(reference, &mut self.source_budget.borrow_mut(), cancel)
                .map_err(|error| GraphOutput::error(error.code, error.message))
        })?;
        let projected =
            source_evidence::project_origin(&origin, &mut self.source_budget.borrow_mut(), cancel)
                .map_err(|error| GraphOutput::error(error.code, error.message))?;
        Ok(json!(projected))
    }

    pub(super) fn close_sources(
        &self,
        output: &mut GraphOutput,
        cancel: &dyn CancellationCheck,
    ) -> Result<(), GraphOutput> {
        let units = source_evidence::referenced_units(
            &output.value,
            &mut self.source_budget.borrow_mut(),
            cancel,
        )
        .map_err(|error| GraphOutput::error(error.code, error.message))?;
        if units.is_empty() {
            return Ok(());
        }
        let descriptors = self.with_sources(cancel, |sources| {
            source_evidence::project_descriptors(
                &sources.descriptors,
                units.iter().map(String::as_str),
                &mut self.source_budget.borrow_mut(),
                cancel,
            )
            .map_err(|error| GraphOutput::error(error.code, error.message))
        })?;
        output.value["sourceEvidence"] = json!(descriptors);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::cancel::NeverCancel;
    use octocode_engine::text::analysis::{Contribution, UnmappedReason};

    #[test]
    fn compact_identity_requires_the_complete_single_raw_utf8_representation() {
        let table = super::super::super::format::evidence_tests::source_fixture();
        let origin = table
            .origin(
                0,
                &mut source_evidence::ProjectionBudget::default(),
                &NeverCancel,
            )
            .unwrap();
        let descriptors = table.descriptors;
        let digest = "a".repeat(64);
        assert!(is_identity(&origin, &descriptors, "a.py", &digest));
        assert!(!is_identity(&origin, &descriptors, "other.py", &digest));
        assert!(!is_identity(&origin, &descriptors, "a.py", &"b".repeat(64)));

        let mut alternatives = Vec::new();
        let mut bom = descriptors.clone();
        bom.snapshots[0].bom_bytes = 3;
        alternatives.push(("UTF-8 BOM", origin.clone(), bom));
        let mut utf16 = descriptors.clone();
        utf16.snapshots[0].encoding = Encoding::Utf16Le;
        utf16.snapshots[0].bom_bytes = 2;
        alternatives.push(("UTF-16", origin.clone(), utf16));
        let mut partial = descriptors.clone();
        partial.units[0].regions = vec![ByteRange::new(0, 8)];
        alternatives.push(("partial analyzed region", origin.clone(), partial));
        let mut multiple = descriptors.clone();
        multiple.representations[0]
            .source_ids
            .push("another-source".into());
        alternatives.push(("multiple sources", origin.clone(), multiple));
        let mut changed = descriptors.clone();
        changed.representations[0].source_digest = "b".repeat(64);
        alternatives.push(("different representation digest", origin.clone(), changed));
        let mut resized = descriptors.clone();
        resized.representations[0].byte_length += 1;
        alternatives.push(("different representation length", origin.clone(), resized));
        let mut excluded = origin.clone();
        excluded.segments[0].analysis = AnalysisMembership::Excluded;
        alternatives.push(("excluded range", excluded, descriptors.clone()));
        let mut partitioned = origin.clone();
        partitioned.segments.push(partitioned.segments[0].clone());
        alternatives.push(("multiple segments", partitioned, descriptors.clone()));
        let OriginBranch::Copied(raw) = &origin.segments[0].branches[0] else {
            panic!("identity fixture");
        };
        for (name, branch) in [
            ("transcoded", OriginBranch::Transcoded(raw.clone())),
            (
                "derived",
                OriginBranch::Derived(vec![Contribution::Source(raw.clone())]),
            ),
            (
                "unmapped",
                OriginBranch::Unmapped(UnmappedReason::Synthetic),
            ),
        ] {
            let mut altered = origin.clone();
            altered.segments[0].branches = vec![branch];
            alternatives.push((name, altered, descriptors.clone()));
        }
        let mut shifted = origin.clone();
        shifted.segments[0].source_range = ByteRange::new(1, 9);
        alternatives.push(("different raw offsets", shifted, descriptors.clone()));
        let mut branched = origin.clone();
        branched.segments[0]
            .branches
            .push(OriginBranch::Copied(raw.clone()));
        alternatives.push(("multiple contributors", branched, descriptors.clone()));
        for (name, candidate, sources) in alternatives {
            assert!(
                !is_identity(&candidate, &sources, "a.py", &digest),
                "{name}"
            );
        }
    }

    #[test]
    fn graph_projection_overflow_returns_only_a_terminal_error() {
        let mut graph = Graph::default();
        graph.t.sources = super::super::super::format::evidence_tests::source_fixture();
        graph.source_budget = std::cell::RefCell::new(source_evidence::ProjectionBudget::new(
            source_evidence::ProjectionLimits {
                bytes: 1,
                records: 1,
            },
        ));
        let error = graph.origin(0, &NeverCancel).unwrap_err();
        assert_eq!(error.value["errorCode"], "responseTooLarge");
        assert!(error.value.get("sourceOrigin").is_none());
        assert!(error.value.get("sourceEvidence").is_none());
        assert!(error.value.get("results").is_none());
    }
}
