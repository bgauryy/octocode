use super::super::format::sources::SourceTable;
use super::*;
use octocode_engine::text::analysis::{
    AnalysisMembership, Encoding, OriginBranch, OriginProjection, SourceDescriptors,
};
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::response::source_evidence;

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
        && unit.regions.iter().any(|region| {
            region.start_byte <= segment.source_range.start_byte
                && segment.source_range.end_byte <= region.end_byte
        })
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
            Some(Err(message)) => Err(*super::load::decode_failure(
                message,
                &self.manifest,
                cancel,
            )),
            None if id == NONE => Ok(std::borrow::Cow::Borrowed("")),
            None => Err(*super::load::integrity_failure(
                "graph string reference is outside the decoded dictionary",
                &self.manifest,
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

    /// Runs `f` on the snapshot's source table; a selective reader reads and
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
        if let Some(reader) = &self.selective {
            let mut reader = reader.borrow_mut();
            if reference != NONE {
                reader
                    .load_origin(reference, cancel)
                    .map_err(|message| *decode_failure(message, &self.manifest, cancel))?;
            }
            let sources = reader
                .sources(cancel)
                .map_err(|message| *decode_failure(message, &self.manifest, cancel))?;
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

    /// Source evidence is read from an immutable snapshot. Authorize each path
    /// on the delivered page under today's policy, including after a symlink
    /// or allowed root changes. Historical files may have been deleted.
    pub(super) fn authorize_source_page(
        &self,
        page: &Value,
        paths: &PathPolicy,
        occurrence_rows: bool,
    ) -> Result<(), GraphOutput> {
        let descriptors = page["sourceEvidence"]["snapshots"].as_array();
        if !occurrence_rows && descriptors.is_none() {
            return Ok(());
        }
        let root = Path::new(self.manifest["root"].as_str().unwrap_or_default());
        let workspace = Path::new(self.manifest["workspace"].as_str().unwrap_or_default());
        let base = if !workspace.as_os_str().is_empty() && root.starts_with(workspace) {
            workspace
        } else {
            root
        };
        let snapshots = descriptors.into_iter().flatten();
        let occurrences = page["rows"]
            .as_array()
            .filter(|_| occurrence_rows)
            .into_iter()
            .flatten();
        let policy = paths.withholding_credentials();
        let mut checked = std::collections::HashSet::new();
        for path in snapshots
            .chain(occurrences)
            .filter_map(|row| row["path"].as_str())
        {
            if !checked.insert(path) {
                continue;
            }
            // Stored descriptors can name an absolute source, whereas an
            // occurrence normally names it relative to the displayed root.
            let candidate = base.join(path);
            let in_scope = candidate.strip_prefix(root).is_ok_and(|tail| {
                !tail.as_os_str().is_empty()
                    && tail
                        .components()
                        .all(|part| matches!(part, std::path::Component::Normal(_)))
            });
            if !in_scope
                || self.manifest["fileScope"]
                    .as_str()
                    .is_some_and(|file| candidate != Path::new(file))
            {
                return Err(GraphOutput::error(
                    ErrorCode::Policy,
                    "Snapshot source evidence lies outside its scanned root.",
                ));
            }
            policy.validate_location(candidate).map_err(|error| {
                GraphOutput::error(
                    error.shared_code(),
                    "The current path policy withholds a source on this evidence page.",
                )
            })?;
        }
        Ok(())
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
        self.rebase_paths(&mut output.value["sourceEvidence"]);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
