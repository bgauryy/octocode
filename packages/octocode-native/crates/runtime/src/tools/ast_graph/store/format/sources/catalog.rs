use super::*;
use budget::{Budget, Input, invalid};
use octocode_engine::text::analysis::{
    AnalysisOperation, AnalysisUnit, Encoding, Producer, SourceSnapshot, TextRepresentation,
};

pub(super) fn source_index(value: &SourceDescriptors, id: &str) -> Result<u32, SourceError> {
    index(
        value
            .snapshots
            .binary_search_by(|row| row.id.as_str().cmp(id)),
    )
}
pub(super) fn unit_index(value: &SourceDescriptors, id: &str) -> Result<u32, SourceError> {
    index(value.units.binary_search_by(|row| row.id.as_str().cmp(id)))
}
fn representation_index(value: &SourceDescriptors, id: &str) -> Result<u32, SourceError> {
    index(
        value
            .representations
            .binary_search_by(|row| row.id.as_str().cmp(id)),
    )
}
fn index(value: Result<usize, usize>) -> Result<u32, SourceError> {
    value
        .ok()
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| invalid("dangling source descriptor reference"))
}
fn producer(input: &mut Input<'_, '_, '_>) -> Result<Producer, SourceError> {
    Ok(Producer {
        id: input.text()?,
        version: input.text()?,
    })
}
fn reference<'a>(
    input: &mut Input<'_, '_, '_>,
    ids: impl FnOnce(usize) -> Option<&'a str>,
) -> Result<String, SourceError> {
    let index = input.number()? as usize;
    let id = ids(index).ok_or_else(|| invalid("source descriptor index out of bounds"))?;
    input.budget.string(id)
}

pub(super) fn read(input: &mut Input<'_, '_, '_>) -> Result<SourceDescriptors, SourceError> {
    let count = input.count()?;
    let mut snapshots = input.budget.vector(count)?;
    for _ in 0..count {
        let id = input.text()?;
        let path = input.text()?;
        let digest = input.text()?;
        let byte_length = input.number()?;
        let encoding = match input.tag(3)? {
            0 => Encoding::Utf8,
            1 => Encoding::Utf16Le,
            _ => Encoding::Utf16Be,
        };
        let bom_bytes = input.number()?;
        snapshots.push(SourceSnapshot {
            id,
            path,
            digest,
            byte_length,
            encoding,
            bom_bytes,
        });
    }
    let count = input.count()?;
    let mut representations = input.budget.vector(count)?;
    for _ in 0..count {
        let id = input.text()?;
        let source_digest = input.text()?;
        let byte_length = input.number()?;
        let producer = producer(input)?;
        let count = input.count()?;
        let mut source_ids = input.budget.vector(count)?;
        for _ in 0..count {
            source_ids.push(reference(input, |index| {
                snapshots.get(index).map(|row| row.id.as_str())
            })?);
        }
        representations.push(TextRepresentation {
            id,
            source_digest,
            byte_length,
            producer,
            source_ids,
        });
    }
    let count = input.count()?;
    let mut units = input.budget.vector(count)?;
    for _ in 0..count {
        let id = input.text()?;
        let adapter = producer(input)?;
        let operation = match input.tag(5)? {
            0 => AnalysisOperation::StructuralSearch,
            1 => AnalysisOperation::SyntaxTree,
            2 => AnalysisOperation::SignatureOutline,
            3 => AnalysisOperation::Declarations,
            _ => AnalysisOperation::GraphFacts,
        };
        let representation_id = reference(input, |index| {
            representations.get(index).map(|row| row.id.as_str())
        })?;
        let count = input.count()?;
        let mut regions = input.budget.vector(count)?;
        for _ in 0..count {
            regions.push(input.range()?);
        }
        units.push(AnalysisUnit {
            id,
            adapter,
            operation,
            representation_id,
            regions,
        });
    }
    Ok(SourceDescriptors {
        snapshots,
        representations,
        units,
    })
}

pub(super) fn write(
    value: &SourceDescriptors,
    out: &mut Vec<u8>,
    budget: &mut Budget<'_>,
) -> Result<(), SourceError> {
    budget.put(out, value.snapshots.len())?;
    for source in &value.snapshots {
        budget.text(out, &source.id)?;
        budget.text(out, &source.path)?;
        budget.text(out, &source.digest)?;
        budget.put(out, source.byte_length as usize)?;
        budget.put(
            out,
            match source.encoding {
                Encoding::Utf8 => 0,
                Encoding::Utf16Le => 1,
                Encoding::Utf16Be => 2,
            },
        )?;
        budget.put(out, source.bom_bytes as usize)?;
    }
    budget.put(out, value.representations.len())?;
    for row in &value.representations {
        budget.text(out, &row.id)?;
        budget.text(out, &row.source_digest)?;
        budget.put(out, row.byte_length as usize)?;
        budget.text(out, &row.producer.id)?;
        budget.text(out, &row.producer.version)?;
        budget.put(out, row.source_ids.len())?;
        for id in &row.source_ids {
            budget.put(out, source_index(value, id)? as usize)?;
        }
    }
    budget.put(out, value.units.len())?;
    for unit in &value.units {
        budget.text(out, &unit.id)?;
        budget.text(out, &unit.adapter.id)?;
        budget.text(out, &unit.adapter.version)?;
        budget.put(
            out,
            match unit.operation {
                AnalysisOperation::StructuralSearch => 0,
                AnalysisOperation::SyntaxTree => 1,
                AnalysisOperation::SignatureOutline => 2,
                AnalysisOperation::Declarations => 3,
                AnalysisOperation::GraphFacts => 4,
            },
        )?;
        budget.put(
            out,
            representation_index(value, &unit.representation_id)? as usize,
        )?;
        budget.put(out, unit.regions.len())?;
        for region in &unit.regions {
            budget.put(out, region.start_byte as usize)?;
            budget.put(out, region.end_byte as usize)?;
        }
    }
    Ok(())
}

/// Fresh decoder allocation costs; kept beside the typed constructor and
/// checked against actual admission in roundtrip boundary tests.
pub(super) fn admit_read(
    value: &SourceDescriptors,
    budget: &mut Budget<'_>,
) -> Result<(), SourceError> {
    budget.vector_cost::<SourceSnapshot>(value.snapshots.len())?;
    for row in &value.snapshots {
        for text in [&row.id, &row.path, &row.digest] {
            budget.charge(text.len(), 1)?;
        }
    }
    budget.vector_cost::<TextRepresentation>(value.representations.len())?;
    for row in &value.representations {
        for text in [
            &row.id,
            &row.source_digest,
            &row.producer.id,
            &row.producer.version,
        ] {
            budget.charge(text.len(), 1)?;
        }
        budget.vector_cost::<String>(row.source_ids.len())?;
        for id in &row.source_ids {
            budget.charge(id.len(), 1)?;
        }
    }
    budget.vector_cost::<AnalysisUnit>(value.units.len())?;
    for row in &value.units {
        for text in [
            &row.id,
            &row.adapter.id,
            &row.adapter.version,
            &row.representation_id,
        ] {
            budget.charge(text.len(), 1)?;
        }
        budget.vector_cost::<ByteRange>(row.regions.len())?;
    }
    Ok(())
}
