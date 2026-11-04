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
    // The two source sections are encoded together, when the first is written.
    let mut origins = None;
    if !t.stamps.is_empty() {
        tags.push(*b"FSTM");
    }
    let start = (HEADER_LEN + tags.len() * TABLE_ENTRY_LEN).next_multiple_of(8);
    let mut offset = write_section(output, 0, &vec![0; start], limit, cancel)?;
    let mut directory = Vec::with_capacity(tags.len() * TABLE_ENTRY_LEN);
    for tag in &tags {
        cancel.check().map_err(ToolError::cancelled)?;
        if tag == b"STRE" {
            let texts = encoder.evidence_texts();
            let mut section = BlockedSection::begin(output, offset, texts.len(), limit, cancel)?;
            blocks::string_blocks(texts, &mut |body| section.body(&body))?;
            offset = section.finish(tag, &mut directory)?;
            continue;
        }
        let flat = match tag {
            b"STRS" => encoder.record_strings(),
            b"NODE" => encoder.nodes(),
            b"LOOK" => super::super::selective::encode_lookup(encoder).map_err(invalid_graph)?,
            b"EDGE" => encoder.edges(),
            b"KEYX" => varints(&t.key_index),
            b"NAMX" => varints(&t.name_index),
            b"DIAG" => encoder.diagnostics(),
            b"WITN" => encoder.witnesses(),
            b"UNRS" => encoder.unresolved(),
            b"DECL" => encoder.declarations(),
            b"SRCE" => {
                let (catalog, pool) =
                    sources::encode_parts(&t.sources, cancel).map_err(ToolError::source)?;
                origins = Some(pool);
                catalog
            }
            b"SRCO" => origins.take().unwrap_or_default(),
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
        if blocks::blocked(tag) {
            let rows = Reader::new(&flat, "encoder records")
                .count()
                .map_err(invalid_graph)?;
            let mut section = BlockedSection::begin(output, offset, rows, limit, cancel)?;
            blocks::record_blocks(tag, &flat, &mut |body| section.body(&body))?;
            offset = section.finish(tag, &mut directory)?;
            continue;
        }
        directory.extend(tag);
        directory.extend(offset.to_le_bytes());
        directory.extend((flat.len() as u64).to_le_bytes());
        directory.extend(blocks::digest(tag, &flat).map_err(invalid_graph)?);
        offset = write_section(output, offset, &flat, limit, cancel)?;
    }
    let digest = octocode_engine::digest::sha256_digest(&directory);
    output.seek(SeekFrom::Start(0)).map_err(io_error)?;
    output.write_all(MAGIC).map_err(io_error)?;
    output
        .write_all(&FORMAT_VERSION.to_le_bytes())
        .map_err(io_error)?;
    let sections = u32::try_from(tags.len())
        .map_err(|_| invalid_graph("graph section count overflows its u32 header".into()))?;
    output
        .write_all(&sections.to_le_bytes())
        .map_err(io_error)?;
    output.write_all(&digest).map_err(io_error)?;
    output.write_all(&directory).map_err(io_error)?;
    let written = usize::try_from(offset)
        .map_err(|_| invalid_graph("graph snapshot size overflows this platform".into()))?;
    Ok((written, hex::encode(digest)))
}

/// One blocked section on its way to the file: each block's body is written
/// as it is built, past the directory's reserved place; the directory, which
/// commits to every body, is written last.
struct BlockedSection<'o, O> {
    output: &'o mut O,
    start: u64,
    end: u64,
    limit: u64,
    directory: Vec<u8>,
    cancel: &'o dyn CancellationCheck,
}

impl<'o, O: std::io::Write + std::io::Seek> BlockedSection<'o, O> {
    fn begin(
        output: &'o mut O,
        start: u64,
        rows: usize,
        limit: u64,
        cancel: &'o dyn CancellationCheck,
    ) -> Result<Self, ToolError> {
        let directory = blocks::directory_start(rows)?;
        let length = blocks::PREFIX + rows.div_ceil(blocks::ROWS) * blocks::ENTRY;
        checked_end(start, length, limit)?;
        let end = start + length as u64;
        output
            .seek(std::io::SeekFrom::Start(end))
            .map_err(io_error)?;
        Ok(Self {
            output,
            start,
            end,
            limit,
            directory,
            cancel,
        })
    }

    fn body(&mut self, body: &blocks::Body<'_>) -> Result<(), ToolError> {
        let end = self
            .end
            .checked_add(body.bytes.len() as u64)
            .filter(|end| *end <= self.limit)
            .ok_or_else(|| over_limit(self.limit))?;
        blocks::directory_entry(&mut self.directory, body, self.end - self.start);
        write_chunks(self.output, body.bytes, self.cancel)?;
        self.end = end;
        Ok(())
    }

    /// Writes the directory and the padding, adds the section to the outer
    /// `table`, and returns where the next section starts.
    fn finish(self, tag: &[u8; 4], table: &mut Vec<u8>) -> Result<u64, ToolError> {
        use std::io::SeekFrom;
        let length = usize::try_from(self.end - self.start)
            .map_err(|_| invalid_graph("graph section size overflows this platform".into()))?;
        let (next, padding) = checked_end(self.start, length, self.limit)?;
        self.output
            .seek(SeekFrom::Start(self.start))
            .map_err(io_error)?;
        self.output.write_all(&self.directory).map_err(io_error)?;
        self.output
            .seek(SeekFrom::Start(self.end))
            .map_err(io_error)?;
        self.output
            .write_all(&[0; 8][..padding])
            .map_err(io_error)?;
        table.extend(tag);
        table.extend(self.start.to_le_bytes());
        table.extend((length as u64).to_le_bytes());
        table.extend(octocode_engine::digest::sha256_digest(&self.directory));
        Ok(next)
    }
}

fn write_chunks(
    output: &mut impl std::io::Write,
    bytes: &[u8],
    cancel: &dyn CancellationCheck,
) -> Result<(), ToolError> {
    for chunk in bytes.chunks(64 * 1024) {
        cancel.check().map_err(ToolError::cancelled)?;
        output.write_all(chunk).map_err(io_error)?;
    }
    Ok(())
}

fn write_section(
    output: &mut impl std::io::Write,
    offset: u64,
    bytes: &[u8],
    limit: u64,
    cancel: &dyn CancellationCheck,
) -> Result<u64, ToolError> {
    let (end, padding) = checked_end(offset, bytes.len(), limit)?;
    write_chunks(output, bytes, cancel)?;
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
        .ok_or_else(|| over_limit(limit))?;
    Ok((end, padding))
}

fn over_limit(limit: u64) -> ToolError {
    ToolError::new(
        octocode_contract_types::ErrorCode::ScopeTooBroad,
        format!("encoded graph exceeds the {limit}-byte snapshot limit; ingest a narrower scope"),
    )
}

fn invalid_graph(message: String) -> ToolError {
    ToolError::new(octocode_contract_types::ErrorCode::Decode, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::cancel::NeverCancel;

    /// Each block reaches the file as it is built, so a section is never held
    /// whole; the directory fills its reserved place last.
    #[test]
    fn a_blocked_section_writes_each_body_as_it_is_built() {
        let mut output = std::io::Cursor::new(Vec::new());
        let body = |first, count, bytes| blocks::Body {
            first,
            last: first + count - 1,
            count,
            bytes,
        };
        let mut section = BlockedSection::begin(&mut output, 8, 257, 1 << 20, &NeverCancel)
            .expect("two blocks fit");
        let start = 8 + blocks::PREFIX + 2 * blocks::ENTRY;
        section.body(&body(0, 256, b"first")).expect("first block");
        assert_eq!(&section.output.get_ref()[start..], b"first");
        section
            .body(&body(256, 1, b"second"))
            .expect("second block");
        let mut table = Vec::new();
        let next = section.finish(b"NODE", &mut table).expect("finish");
        let bytes = output.into_inner();
        assert_eq!(&bytes[start..start + 11], b"firstsecond");
        assert_eq!(next as usize % 8, 0, "padded");
        assert_eq!(bytes.len() as u64, next);
        let section = &bytes[8..start + 11];
        let (rows, parsed) = blocks::parse(section, section.len() as u64).expect("directory");
        assert_eq!(rows, 257);
        assert_eq!(parsed.len(), 2);
        assert_eq!(
            parsed[1].offset,
            (blocks::PREFIX + 2 * blocks::ENTRY + 5) as u64
        );
        assert_eq!(
            table[20..],
            blocks::digest(b"NODE", section).expect("digest"),
            "the outer table commits to the directory"
        );
    }

    #[test]
    fn a_block_past_the_snapshot_limit_never_reaches_the_file() {
        let limit = (blocks::PREFIX + blocks::ENTRY + 8) as u64;
        let mut output = std::io::Cursor::new(Vec::new());
        let mut section =
            BlockedSection::begin(&mut output, 0, 1, limit, &NeverCancel).expect("directory fits");
        let error = section
            .body(&blocks::Body {
                first: 0,
                last: 0,
                count: 1,
                bytes: &[1; 9],
            })
            .expect_err("over limit");
        assert_eq!(
            error.code,
            octocode_contract_types::ErrorCode::ScopeTooBroad
        );
        assert!(output.into_inner().is_empty());
    }

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
