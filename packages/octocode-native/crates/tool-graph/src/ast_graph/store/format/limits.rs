//! The encoder admits only table sizes accepted by the decoder. Row counts
//! are checked on the projection, before redaction (which rewrites strings,
//! never rows); string bytes once the redacted strings are encoded.
use super::*;

pub(super) fn validate(encoder: &writer::Encoder<'_>, limits: &Limits) -> Result<(), String> {
    validate_counts(encoder.tables, limits)?;
    validate_strings(encoder, limits)
}

/// The first table of `t` with more rows than a reader of `limits` admits.
pub(crate) fn validate_counts(t: &GraphTables, limits: &Limits) -> Result<(), String> {
    let items = limits.table_items;
    for (section, count, width) in [
        ("NODE", t.nodes.len(), 1),
        ("KEYX", t.key_index.len(), 1),
        ("NAMX", t.name_index.len(), 1),
        ("DIAG", t.diagnostics.len(), 1),
        ("WITN", t.witnesses.len(), 1),
        ("UNRS", t.unresolved.len(), 1),
        ("DECL", t.declarations.len(), 1),
        ("FDIG", t.digests.len(), 2),
        ("FCMP", t.components.len(), 4),
        ("ENTR", t.entries.len(), 2),
        ("FSTM", t.stamps.len(), 1),
    ] {
        if count.checked_mul(width).is_none_or(|count| count > items) {
            return Err(format!(
                "graph {section} table exceeds the reader's {items}-item limit; this is a terminal snapshot bound, ingest a narrower scope"
            ));
        }
    }
    if t.edges.len() > limits.edge_rows() {
        return Err(format!(
            "graph EDGE table exceeds the reader's {}-edge limit; this is a terminal snapshot bound, ingest a narrower scope",
            limits.edge_rows(),
        ));
    }
    Ok(())
}

/// The dictionaries' count and expanded bytes as readers admit them: the
/// record strings (`STRS`) whole, the evidence strings (`STRE`) one block
/// at a time.
fn validate_strings(encoder: &writer::Encoder<'_>, limits: &Limits) -> Result<(), String> {
    let (items, string_bytes) = (limits.table_items, limits.string_bytes);
    // Record strings are read whole; evidence strings a block at a time, so
    // the two together are bounded only by the `u32` id every column holds.
    if encoder.records > items || u32::try_from(encoder.texts.len()).is_err() {
        return Err(format!(
            "graph STRS table exceeds the reader's {items}-item limit; this is a terminal snapshot bound, ingest a narrower scope"
        ));
    }
    let fits = |texts: &[&str]| {
        texts
            .iter()
            .try_fold(0_usize, |total, text| {
                total
                    .checked_add(text.len())
                    .filter(|bytes| *bytes <= string_bytes)
            })
            .is_some()
    };
    let records = &encoder.texts[..encoder.records];
    if !fits(records) || !encoder.evidence_texts().chunks(blocks::ROWS).all(fits) {
        return Err(format!(
            "graph STRS table exceeds the reader's {string_bytes}-byte expanded string limit; this is a terminal snapshot bound, ingest a narrower scope"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
