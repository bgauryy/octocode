//! Reading and writing the source table: the `SRCE` catalog and the `SRCO`
//! origin pool, decoded under the section budget and validated on read.

use super::*;

/// The source table from `SRCE` (the catalog) and the unpacked `SRCO` (the
/// origin pool), which together are the encoded table.
pub(in super::super) fn read(
    sections: &Sections,
    tables: &mut GraphTables,
    admission: Admission,
    cancel: &dyn CancellationCheck,
) -> Result<(), String> {
    let catalog = sections.get(b"SRCE", "SRCE")?;
    let origins = sections.get(b"SRCO", "SRCO")?;
    let mut bytes = Vec::with_capacity(catalog.bytes.len() + origins.bytes.len());
    bytes.extend_from_slice(catalog.bytes);
    bytes.extend_from_slice(origins.bytes);
    tables.sources = decode_admitted(&bytes, &mut budget::Budget::new(cancel, &LIMITS), admission)
        .map_err(|error| format!("graph source correspondence is invalid: {error}"))?;
    Ok(())
}

/// The catalog alone, with an origin pool that reads its blocks on demand
/// ([`SourceTable::insert_origin_block`]); `origins` is the pool's size.
pub(in super::super) fn read_catalog(
    bytes: &[u8],
    origins: usize,
    cancel: &dyn CancellationCheck,
) -> Result<SourceTable, String> {
    let invalid = |error: SourceError| format!("graph source correspondence is invalid: {error}");
    let mut budget = budget::Budget::new(cancel, &LIMITS);
    let mut input = budget::Input::new(bytes, &mut budget);
    let descriptors = catalog::read(&mut input).map_err(invalid)?;
    input.finish().map_err(invalid)?;
    check_catalog(&descriptors, cancel).map_err(invalid)?;
    Ok(SourceTable {
        descriptors,
        origins: pool::OriginPool::sparse(origins),
    })
}

/// Sorted, unique descriptor ids and a valid catalog.
fn check_catalog(
    descriptors: &SourceDescriptors,
    cancel: &dyn CancellationCheck,
) -> Result<(), SourceError> {
    sorted_ids(
        descriptors.snapshots.iter().map(|row| row.id.as_str()),
        cancel,
    )?;
    sorted_ids(
        descriptors
            .representations
            .iter()
            .map(|row| row.id.as_str()),
        cancel,
    )?;
    sorted_ids(descriptors.units.iter().map(|row| row.id.as_str()), cancel)?;
    let poll = super::Poll::new(cancel);
    let stop = || poll.stopped();
    SourceCatalog::new(descriptors, &mut map_budget(cancel, &stop))?;
    Ok(())
}
#[cfg(test)]
pub(in super::super) fn decode(
    bytes: &[u8],
    cancel: &dyn CancellationCheck,
) -> Result<SourceTable, SourceError> {
    decode_with_budget(bytes, &mut budget::Budget::new(cancel, &LIMITS))
}
#[cfg(test)]
pub(super) fn decode_with_budget(
    bytes: &[u8],
    budget: &mut budget::Budget<'_>,
) -> Result<SourceTable, SourceError> {
    decode_admitted(bytes, budget, Admission::Every)
}
pub(super) fn decode_admitted(
    bytes: &[u8],
    budget: &mut budget::Budget<'_>,
    admission: Admission,
) -> Result<SourceTable, SourceError> {
    let cancel = budget.cancel;
    let mut input = budget::Input::new(bytes, budget);
    let descriptors = catalog::read(&mut input)?;
    let origins = match admission {
        Admission::Every => {
            sorted_ids(
                descriptors.snapshots.iter().map(|row| row.id.as_str()),
                cancel,
            )?;
            sorted_ids(
                descriptors
                    .representations
                    .iter()
                    .map(|row| row.id.as_str()),
                cancel,
            )?;
            sorted_ids(descriptors.units.iter().map(|row| row.id.as_str()), cancel)?;
            pool::OriginPool::read(&mut input, &descriptors)?
        }
        Admission::Accessed => {
            check_catalog(&descriptors, cancel)?;
            pool::OriginPool::read_index(&mut input, &descriptors)?
        }
    };
    Ok(SourceTable {
        descriptors,
        origins,
    })
}
#[cfg(test)]
pub(in super::super) fn encode(
    sources: &SourceTable,
    cancel: &dyn CancellationCheck,
) -> Result<Vec<u8>, SourceError> {
    let (mut catalog, origins) = encode_parts(sources, cancel)?;
    catalog.extend(origins);
    Ok(catalog)
}

/// The catalog (`SRCE`) and the origin pool (`SRCO`, before packing).
pub(in super::super) fn encode_parts(
    sources: &SourceTable,
    cancel: &dyn CancellationCheck,
) -> Result<(Vec<u8>, Vec<u8>), SourceError> {
    let (mut catalog, mut origins) = (Vec::new(), Vec::new());
    let mut budget = budget::Budget::new(cancel, &LIMITS);
    catalog::write(&sources.descriptors, &mut catalog, &mut budget)?;
    sources.origins.write(&mut origins, &mut budget)?;
    Ok((catalog, origins))
}
