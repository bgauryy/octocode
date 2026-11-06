//! Recording whole-source facts and redaction edits during one pass.
use super::*;

impl<'s> Proof<'s> {
    /// A pass over `file`, the source at `path`, that also holds the windows of
    /// `wants` and records what `scan` redacts.
    pub(in super::super) fn new(
        file: &std::fs::File,
        path: &std::path::Path,
        wants: &[Want],
        scan: &'s dyn ContentScan,
    ) -> Result<Self, String> {
        let started = octocode_engine::stamp::settle_clock();
        Ok(Self {
            scan,
            recorder: Recorder::new(path, super::super::sealed::Limits::REAL),
            key_lines: Vec::new(),
            hasher: octocode_engine::digest::HashWriter::default(),
            lines: WindowLines::new(wants, KeyBlockState::default()),
            line: 1,
            pending: false,
            offset: 0,
            next: CHECKPOINT_BYTES,
            checkpoints: vec![Checkpoint {
                offset: 0,
                line: 1,
                block: KeyBlockState::default(),
            }],
            stamp: stamp(file)?,
            started,
            names: SourceNames::for_path(&path.to_string_lossy()),
            in_lines: Vec::new(),
            long_lines: Vec::new(),
            line_at: 0,
            next_in_line: 0,
        })
    }

    /// Take the source's next `bytes`.
    pub(in super::super) fn feed(&mut self, bytes: &[u8]) {
        self.hasher.update(bytes);
        self.names.feed(bytes);
        self.recorder.feed(bytes, self.scan);
        let mut rest = bytes;
        while !rest.is_empty() {
            let (piece, newline) = split_line(rest);
            rest = &rest[piece.len()..];
            self.offset += piece.len() as u64;
            self.pending = !newline;
            self.lines.push(piece, self.line);
            if !newline
                && self.offset - self.line_at >= CHECKPOINT_BYTES
                && self.offset >= self.next_in_line
            {
                self.in_lines.push(self.lines.in_line(self.line));
                self.next_in_line = self.offset + CHECKPOINT_BYTES;
            }
            if newline {
                self.close_line();
                self.line += 1;
                if self.offset >= self.next {
                    self.checkpoints.push(Checkpoint {
                        offset: self.offset,
                        line: self.line,
                        block: self.lines.block,
                    });
                    self.next = self.offset + CHECKPOINT_BYTES;
                }
            }
        }
    }

    fn close_line(&mut self) {
        let hidden = self.lines.finish(self.line);
        let bytes = self.offset - self.line_at;
        if let Some(keyed) = self.lines.replaced {
            let (from, to) = (self.line_at, self.offset);
            self.key_lines.push((
                octocode_engine::num::saturating_usize(from)
                    ..octocode_engine::num::saturating_usize(to),
                keyed,
            ));
        }
        if bytes > CHECKPOINT_BYTES && !hidden {
            self.long_lines.push(LongLineExtent {
                line: self.line,
                bytes: octocode_engine::num::saturating_usize(bytes),
            });
        }
        self.line_at = self.offset;
    }

    /// The pass once the source ended, or the scanner's refusal of it.
    pub(in super::super) fn finish(mut self) -> Result<Pass, String> {
        if self.pending {
            self.close_line();
        }
        let edits = self
            .recorder
            .finish(self.scan)
            .map_err(|(_, message)| message)?;
        Ok(Pass {
            proven: Proven {
                scanned: outside_key_lines(edits, &self.key_lines),
                key_lines: self.key_lines.into(),
                sealed: OnceLock::new(),
                digest: hex::encode(self.hasher.finalize()),
                checkpoints: self.checkpoints,
                stamp: self.stamp,
                started: self.started,
                named: self.names.finish(),
                judged: OnceLock::new(),
                in_lines: self.in_lines,
                long_lines: self.long_lines,
            },
            lines: self.lines,
            total_lines: if self.pending {
                self.line
            } else {
                self.line - 1
            },
        })
    }
}

/// `edits` less their bytes on lines a private-key block replaces: such a line
/// shows only its placeholder. An edit cut by one keeps its placeholder.
pub(super) fn outside_key_lines(
    edits: Vec<Edit>,
    key_lines: &[(std::ops::Range<usize>, usize)],
) -> Vec<Edit> {
    if key_lines.is_empty() {
        return edits;
    }
    let mut out = Vec::with_capacity(edits.len());
    for edit in edits {
        let mut range = edit.range.clone();
        let first = key_lines.partition_point(|(line, _)| line.end <= range.start);
        for (line, _) in key_lines[first..]
            .iter()
            .take_while(|(line, _)| line.start < edit.range.end)
        {
            if line.start <= range.start {
                range.start = range.start.max(line.end);
            } else {
                // The bytes past this key line stay hidden by the same edit.
                if range.start < line.start {
                    out.push(Edit {
                        range: range.start..line.start,
                        text: edit.text.clone(),
                    });
                }
                range.start = line.end;
            }
        }
        if range.start < range.end {
            out.push(Edit {
                range,
                text: edit.text,
            });
        }
    }
    out
}

/// The bytes of `rest` up to and including its first newline, and whether
/// there was one.
pub(super) fn split_line(rest: &[u8]) -> (&[u8], bool) {
    match rest.iter().position(|byte| *byte == b'\n') {
        Some(index) => (&rest[..=index], true),
        None => (rest, false),
    }
}
