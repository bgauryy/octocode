//! Stream a staged snapshot one section at a time.
use super::*;
/// Writes sections one at a time within `limits`; publication owns the
/// staged path and final rename.
pub(crate) fn write_for_publish(
    tables: &GraphTables,
    path: &std::path::Path,
    limits: &Limits,
    cancel: &dyn CancellationCheck,
) -> Result<(usize, String), ToolError> {
    let encoder = checked_encoder(tables, limits, cancel)?;
    let mut file = std::fs::File::create(path).map_err(io_error)?;
    let result = write_snapshot(&encoder, &mut file, limits, cancel)?;
    file.sync_all().map_err(io_error)?;
    Ok(result)
}

fn io_error(error: std::io::Error) -> ToolError {
    ToolError::new(
        octocode_contract_types::ErrorCode::FileAccessFailed,
        format!("cannot write graph snapshot: {error}"),
    )
}

pub(super) fn write_snapshot(
    encoder: &Encoder<'_>,
    output: &mut (impl std::io::Write + std::io::Seek),
    limits: &Limits,
    cancel: &dyn CancellationCheck,
) -> Result<(usize, String), ToolError> {
    let limit = limits.snapshot_bytes as u64;
    use std::io::SeekFrom;
    let t = encoder.tables;
    let mut tags = vec![
        *b"STRS", *b"STRE", *b"NODE", *b"LOOK", *b"EDGE", *b"KEYX", *b"NAMX", *b"DIAG", *b"WITN",
        *b"UNRS", *b"SRCE", *b"SRCO", *b"DECL", *b"FDIG", *b"FCMP", *b"ENTR",
    ];
    let (catalog, origins) =
        sources::encode_parts(&t.sources, cancel).map_err(ToolError::source)?;
    let mut sources = Some((catalog, origins));
    if !t.stamps.is_empty() {
        tags.push(*b"FSTM");
    }
    let start = (HEADER_LEN + tags.len() * TABLE_ENTRY_LEN).next_multiple_of(8);
    let mut offset = write_section(output, 0, &vec![0; start], limit, cancel)?;
    let mut directory = Vec::with_capacity(tags.len() * TABLE_ENTRY_LEN);
    for tag in &tags {
        cancel.check().map_err(ToolError::cancelled)?;
        let flat = match tag {
            b"STRS" => encoder.record_strings(),
            b"STRE" => encoder.evidence_strings(),
            b"NODE" => encoder.nodes(),
            b"LOOK" => super::super::selective::encode_lookup(encoder).map_err(invalid_graph)?,
            b"EDGE" => encoder.edges(),
            b"KEYX" => varints(&t.key_index),
            b"NAMX" => varints(&t.name_index),
            b"DIAG" => encoder.diagnostics(),
            b"WITN" => encoder.witnesses(),
            b"UNRS" => encoder.unresolved(),
            b"DECL" => encoder.declarations(),
            b"SRCE" => sources
                .as_mut()
                .map(|parts| std::mem::take(&mut parts.0))
                .unwrap_or_default(),
            b"SRCO" => sources.take().map(|parts| parts.1).unwrap_or_default(),
            b"FDIG" => varints(
                &t.digests
                    .iter()
                    .flat_map(|(node, digest)| [*node, encoder.sid(*digest)])
                    .collect::<Vec<_>>(),
            ),
            b"FCMP" => varints(
                &t.components
                    .iter()
                    .flat_map(|(node, dir, name, meta)| {
                        [
                            *node,
                            encoder.sid(*dir),
                            encoder.sid(*name),
                            encoder.sid(*meta),
                        ]
                    })
                    .collect::<Vec<_>>(),
            ),
            b"ENTR" => varints(
                &t.entries
                    .iter()
                    .flat_map(|(node, rule)| [*node, encoder.sid(*rule)])
                    .collect::<Vec<_>>(),
            ),
            b"FSTM" => stamp_section(&t.stamps),
            _ => return Err(invalid_graph("unknown encoder section".into())),
        };
        let bytes = if blocks::blocked(tag) {
            blocks::pack(tag, &flat).map_err(invalid_graph)?
        } else {
            flat
        };
        directory.extend(tag);
        directory.extend(offset.to_le_bytes());
        directory.extend((bytes.len() as u64).to_le_bytes());
        directory.extend(blocks::digest(tag, &bytes).map_err(invalid_graph)?);
        offset = write_section(output, offset, &bytes, limit, cancel)?;
    }
    let digest = Sha256::digest(&directory);
    output.seek(SeekFrom::Start(0)).map_err(io_error)?;
    output.write_all(MAGIC).map_err(io_error)?;
    output
        .write_all(&FORMAT_VERSION.to_le_bytes())
        .map_err(io_error)?;
    output
        .write_all(&(tags.len() as u32).to_le_bytes())
        .map_err(io_error)?;
    output.write_all(&digest).map_err(io_error)?;
    output.write_all(&directory).map_err(io_error)?;
    Ok((offset as usize, hex::encode(digest)))
}

fn write_section(
    output: &mut impl std::io::Write,
    offset: u64,
    bytes: &[u8],
    limit: u64,
    cancel: &dyn CancellationCheck,
) -> Result<u64, ToolError> {
    let (end, padding) = checked_end(offset, bytes.len(), limit)?;
    for chunk in bytes.chunks(64 * 1024) {
        cancel.check().map_err(ToolError::cancelled)?;
        output.write_all(chunk).map_err(io_error)?;
    }
    output.write_all(&[0; 8][..padding]).map_err(io_error)?;
    Ok(end)
}

/// The end of a `length`-byte section at `offset`, padded, within `limit`.
fn checked_end(offset: u64, length: usize, limit: u64) -> Result<(u64, usize), ToolError> {
    let padding = (8 - length % 8) % 8;
    let end = u64::try_from(length)
        .ok()
        .and_then(|length| offset.checked_add(length))
        .and_then(|end| end.checked_add(padding as u64))
        .filter(|end| *end <= limit)
        .ok_or_else(|| {
            ToolError::new(
                octocode_contract_types::ErrorCode::ScopeTooBroad,
                format!(
                    "encoded graph exceeds the {limit}-byte snapshot limit; ingest a narrower scope"
                ),
            )
        })?;
    Ok((end, padding))
}

fn invalid_graph(message: String) -> ToolError {
    ToolError::new(octocode_contract_types::ErrorCode::Decode, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::cancel::NeverCancel;

    #[test]
    fn snapshot_write_limit_includes_padding_before_writing() {
        let limit = LIMITS.snapshot_bytes as u64;
        let mut output = Vec::new();
        assert_eq!(
            write_section(&mut output, limit - 8, &[1], limit, &NeverCancel).expect("last block"),
            limit
        );
        assert_eq!(output, [1, 0, 0, 0, 0, 0, 0, 0]);
        for (offset, bytes) in [(limit, &[1][..]), (limit - 8, &[1; 9][..])] {
            let error = write_section(&mut output, offset, bytes, limit, &NeverCancel)
                .expect_err("over-limit block");
            assert_eq!(
                error.code,
                octocode_contract_types::ErrorCode::ScopeTooBroad
            );
            assert_eq!(output.len(), 8, "rejected bytes never reach the writer");
        }
        assert!(
            checked_end(limit - 1, 1, limit).is_err(),
            "padding is bounded"
        );
        assert!(
            checked_end(u64::MAX, 1, limit).is_err(),
            "offset cannot wrap"
        );
        assert!(
            checked_end(0, usize::MAX, limit).is_err(),
            "length cannot wrap"
        );
        assert_eq!(
            checked_end(limit, 0, limit).expect("empty section"),
            (limit, 0)
        );
    }
}
