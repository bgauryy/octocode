use super::*;
use octocode_engine::text::analysis::{
    AnalysisMembership, Contribution, OriginBranch, OriginSegment, OriginalRange, UnmappedReason,
};

fn reason(input: &mut Input<'_, '_, '_>) -> Result<UnmappedReason, SourceError> {
    Ok(match input.tag(3)? {
        0 => UnmappedReason::Synthetic,
        1 => UnmappedReason::RawSourceUnavailable,
        _ => UnmappedReason::MappingUnavailable,
    })
}
fn raw(
    input: &mut Input<'_, '_, '_>,
    descriptors: &SourceDescriptors,
) -> Result<OriginalRange, SourceError> {
    let source = descriptors
        .snapshots
        .get(input.number()? as usize)
        .ok_or_else(|| invalid("source origin snapshot index out of bounds"))?;
    Ok(OriginalRange {
        source_id: input.budget.string(&source.id)?,
        raw_range: input.range()?,
    })
}
pub(super) fn read(
    input: &mut Input<'_, '_, '_>,
    descriptors: &SourceDescriptors,
) -> Result<(u32, OriginProjection), SourceError> {
    let unit = input.number()?;
    let id = &descriptors
        .units
        .get(unit as usize)
        .ok_or_else(|| invalid("source origin unit index out of bounds"))?
        .id;
    let analysis_unit_id = input.budget.string(id)?;
    let count = input.count()?;
    let mut segments = input.budget.vector(count)?;
    for _ in 0..count {
        let source_range = input.range()?;
        let analysis = match input.tag(2)? {
            0 => AnalysisMembership::Included,
            _ => AnalysisMembership::Excluded,
        };
        let count = input.count()?;
        let mut branches = input.budget.vector(count)?;
        for _ in 0..count {
            let branch = match input.tag(4)? {
                0 => OriginBranch::Copied(raw(input, descriptors)?),
                1 => OriginBranch::Transcoded(raw(input, descriptors)?),
                2 => {
                    let count = input.count()?;
                    let mut values = input.budget.vector(count)?;
                    for _ in 0..count {
                        values.push(match input.tag(2)? {
                            0 => Contribution::Source(raw(input, descriptors)?),
                            _ => Contribution::Unmapped(reason(input)?),
                        });
                    }
                    OriginBranch::Derived(values)
                }
                _ => OriginBranch::Unmapped(reason(input)?),
            };
            branches.push(branch);
        }
        segments.push(OriginSegment {
            source_range,
            analysis,
            branches,
        });
    }
    Ok((
        unit,
        OriginProjection {
            analysis_unit_id,
            segments,
        },
    ))
}
fn put_reason(
    out: &mut Vec<u8>,
    value: UnmappedReason,
    budget: &mut Budget<'_>,
) -> Result<(), SourceError> {
    budget.put(
        out,
        match value {
            UnmappedReason::Synthetic => 0,
            UnmappedReason::RawSourceUnavailable => 1,
            UnmappedReason::MappingUnavailable => 2,
        },
    )
}
fn put_raw(
    out: &mut Vec<u8>,
    value: &OriginalRange,
    descriptors: &SourceDescriptors,
    budget: &mut Budget<'_>,
) -> Result<(), SourceError> {
    budget.put(
        out,
        catalog::source_index(descriptors, &value.source_id)? as usize,
    )?;
    budget.put(out, value.raw_range.start_byte as usize)?;
    budget.put(out, value.raw_range.end_byte as usize)
}
pub(super) fn write(
    out: &mut Vec<u8>,
    origin: &OriginProjection,
    descriptors: &SourceDescriptors,
    budget: &mut Budget<'_>,
) -> Result<(), SourceError> {
    budget.put(
        out,
        catalog::unit_index(descriptors, &origin.analysis_unit_id)? as usize,
    )?;
    budget.put(out, origin.segments.len())?;
    for segment in &origin.segments {
        budget.put(out, segment.source_range.start_byte as usize)?;
        budget.put(out, segment.source_range.end_byte as usize)?;
        budget.put(
            out,
            match segment.analysis {
                AnalysisMembership::Included => 0,
                AnalysisMembership::Excluded => 1,
            },
        )?;
        budget.put(out, segment.branches.len())?;
        for branch in &segment.branches {
            match branch {
                OriginBranch::Copied(value) | OriginBranch::Transcoded(value) => {
                    budget.put(
                        out,
                        usize::from(matches!(branch, OriginBranch::Transcoded(_))),
                    )?;
                    put_raw(out, value, descriptors, budget)?;
                }
                OriginBranch::Derived(values) => {
                    budget.put(out, 2)?;
                    budget.put(out, values.len())?;
                    for value in values {
                        match value {
                            Contribution::Source(value) => {
                                budget.put(out, 0)?;
                                put_raw(out, value, descriptors, budget)?;
                            }
                            Contribution::Unmapped(value) => {
                                budget.put(out, 1)?;
                                put_reason(out, *value, budget)?;
                            }
                        }
                    }
                }
                OriginBranch::Unmapped(value) => {
                    budget.put(out, 3)?;
                    put_reason(out, *value, budget)?;
                }
            }
        }
    }
    Ok(())
}
