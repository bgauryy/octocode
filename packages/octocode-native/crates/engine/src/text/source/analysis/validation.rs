use super::templates::{RepresentationTemplate, UnitTemplate};
use super::*;

impl BoundSource<'_> {
    pub(super) fn validate_template(
        &self,
        handle: RepresentationHandle,
        template: &RepresentationTemplate,
        budget: &mut MapBudget<'_>,
    ) -> Result<(), SourceError> {
        match &template.content {
            RepresentationContent::Root {
                source_index,
                source_digest,
                byte_length,
            } => {
                let source = self
                    .sources
                    .get(*source_index as usize)
                    .ok_or(SourceError::InvalidMap("missing source"))?;
                if source.text().len() != *byte_length as usize
                    || source.text_digest() != source_digest.as_str()
                {
                    return Err(SourceError::InvalidMap("decoded source identity"));
                }
            }
            RepresentationContent::Prefix {
                input, end_byte, ..
            } => self.validate_range(*input, ByteRange::new(0, *end_byte), budget)?,
            RepresentationContent::Mapped { text, segments } => {
                if segments.is_empty() || (text.is_empty() && segments.len() != 1) {
                    return Err(SourceError::InvalidMap("partition"));
                }
                let mut end = 0;
                for segment in segments {
                    budget.segment()?;
                    self.validate_range(handle, segment.range, budget)?;
                    if segment.range.start_byte != end
                        || (segment.range.is_empty() && !text.is_empty())
                        || segment.branches.is_empty()
                    {
                        return Err(SourceError::InvalidMap("partition"));
                    }
                    end = segment.range.end_byte;
                    for branch in &segment.branches {
                        budget.branch()?;
                        match branch {
                            MapBranch::Exact { input, range } => {
                                self.validate_range(*input, *range, budget)?;
                                let expected = text
                                    .get(
                                        segment.range.start_byte as usize
                                            ..segment.range.end_byte as usize,
                                    )
                                    .ok_or(SourceError::InvalidMap("exact output"))?;
                                let mut at = 0;
                                let mut equal = true;
                                self.chunks(
                                    *input,
                                    *range,
                                    &mut |chunk| {
                                        equal &= expected.get(at..at + chunk.len()) == Some(chunk);
                                        at += chunk.len();
                                    },
                                    budget,
                                    0,
                                )?;
                                if !equal || at != expected.len() {
                                    return Err(SourceError::InvalidMap(
                                        "nonidentical exact branch",
                                    ));
                                }
                            }
                            MapBranch::Derived { contributors } => {
                                if contributors.is_empty() {
                                    return Err(SourceError::InvalidMap("empty contributors"));
                                }
                                for contributor in contributors {
                                    budget.contributor()?;
                                    if let MapContribution::Source { input, range } = contributor {
                                        self.validate_range(*input, *range, budget)?;
                                    }
                                }
                            }
                            MapBranch::Unmapped(_) => {}
                        }
                    }
                }
                if end as usize != text.len() {
                    return Err(SourceError::InvalidMap("incomplete partition"));
                }
            }
        }
        Ok(())
    }

    pub(super) fn resolve_unit(
        &self,
        unit: &UnitTemplate,
        budget: &mut MapBudget<'_>,
    ) -> Result<AnalysisUnit, SourceError> {
        if unit.regions.is_empty() {
            return Err(SourceError::InvalidMap("empty analysis regions"));
        }
        let representation = self
            .representations
            .get(unit.representation.0 as usize)
            .ok_or(SourceError::InvalidMap("unit representation"))?;
        let mut previous = 0;
        for (index, &range) in unit.regions.iter().enumerate() {
            self.validate_range(unit.representation, range, budget)?;
            if index > 0 && range.start_byte < previous {
                return Err(SourceError::InvalidMap("overlapping regions"));
            }
            previous = range.end_byte;
        }
        budget.allocate(
            64 + unit.adapter.id.len()
                + unit.adapter.version.len()
                + representation.id.len()
                + unit.regions.len() * std::mem::size_of::<ByteRange>(),
        )?;
        let id = model::identity(&(
            "analysis-unit:1",
            &unit.adapter,
            unit.operation,
            &representation.id,
            &unit.regions,
        ))?;
        Ok(AnalysisUnit {
            id,
            adapter: unit.adapter.clone(),
            operation: unit.operation,
            representation_id: representation.id.clone(),
            regions: unit.regions.clone(),
        })
    }
}
