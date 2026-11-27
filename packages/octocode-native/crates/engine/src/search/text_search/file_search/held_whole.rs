//! A file with a line over the line buffer's bound, searched from its bytes held
//! whole once, so that line is searched like any other.
use super::{
    BinaryDetection, CollectSink, CopiedRead, FILE_TERMS_MAX_BYTES, FileOutcome, FileSearcher,
    MAX_BINARY_PREFIX_BYTES, MatchWork, build_searcher, is_leading_binary,
};
use grep_matcher::Matcher;
use std::path::Path;

/// Whether a search failed because one line outgrew the line buffer's bound.
pub(super) fn outgrew_the_line_buffer(error: &std::io::Error) -> bool {
    error.to_string().contains("allocation limit")
}

impl<M: Matcher> FileSearcher<'_, M> {
    /// [`Self::search`] of a file with a line over the line buffer's bound: the
    /// file's bytes held whole, within the search's file ceiling, once, where the
    /// buffer would have doubled to hold that one line.
    pub(super) fn search_read_whole(
        &mut self,
        file: &std::fs::File,
        path: &Path,
        work: MatchWork,
    ) -> std::io::Result<FileOutcome> {
        // The line buffer grew to its bound before it failed: its bytes are
        // let go, not kept by this worker's searcher for every later file.
        self.searcher = build_searcher(self.opts, self.context, BinaryDetection::quit(b'\x00'));
        let len = file.metadata()?.len();
        let _held = self.admit(usize::try_from(len).unwrap_or(usize::MAX))?;
        let (bytes, filled) = self.read_whole(file, len)?;
        // The rows come from these bytes: the file's digest is of them.
        let held = &bytes.as_slice()[..filled];
        self.read.restart();
        self.read.feed(held);
        if (filled as u64) < len {
            self.read.ended();
        }
        self.search_bytes(super::Searched::own(held), path, work)
    }

    /// The first `len` bytes of the file in one region sized from them (no
    /// growth chain, returned to the system when dropped), read in chunks so a
    /// cancelled search ends the read; how many bytes it holds is the
    /// returned count.
    fn read_whole(
        &self,
        file: &std::fs::File,
        len: u64,
    ) -> std::io::Result<(crate::memory::Region, usize)> {
        use std::io::{Read, Seek, SeekFrom};
        const CHUNK: usize = 8 * 1024 * 1024;
        let mut handle = file;
        handle.seek(SeekFrom::Start(0))?;
        let mut region = crate::memory::Region::new(usize::try_from(len).unwrap_or(0))?;
        let mut filled = 0;
        let room = region.as_mut_slice();
        while filled < room.len() {
            if self.cancelled.is_some_and(|gone| gone()) {
                return Err(super::stopped_read());
            }
            let end = room.len().min(filled + CHUNK);
            match handle.read(&mut room[filled..end]) {
                Ok(0) => break,
                Ok(read) => filled += read,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error),
            }
        }
        Ok((region, filled))
    }

    /// [`Self::search`] of a file's text held whole: its bytes, or the text
    /// its judge serves ([`super::FileSearcher::judged`]).
    pub(super) fn search_bytes(
        &mut self,
        text: super::Searched<'_>,
        path: &Path,
        work: MatchWork,
    ) -> std::io::Result<FileOutcome> {
        let super::Searched { bytes, written } = text;
        let mut sink = CollectSink::new(self.matcher, work, (self.stop, self.held), self.terms);
        sink.cancelled = self.cancelled;
        sink.written = written;
        sink.test_code = self.opts.test_code.as_ref().map(|scope| (scope, path));
        sink.line_scope = self.opts.line_scope.as_ref().map(|scope| (scope, path));
        sink.source = Some(path);
        sink.inspect_deferred = self
            .opts
            .inspect_deferred
            .as_ref()
            .map(|inspect| (inspect, path));
        // An inverted search runs over the text with what the judge wrote made
        // filler, so a line whose only match is a placeholder's spelling is
        // no match; its lines are read from the served text.
        let filled = (self.opts.invert_match.unwrap_or(false) && !written.is_empty())
            .then(|| super::super::written::fill(bytes, written));
        if filled.is_some() {
            sink.inverted_over = Some(bytes);
        }
        self.searcher
            .search_slice(self.matcher, filled.as_deref().unwrap_or(bytes), &mut sink)?;
        let binary_at = sink
            .binary_offset
            .filter(|offset| *offset != 0 && *offset <= MAX_BINARY_PREFIX_BYTES);
        let mut outcome = match binary_at {
            None => FileOutcome::from(sink),
            Some(offset) => {
                let prefix =
                    &bytes[..usize::try_from(offset).map_or(bytes.len(), |at| at.min(bytes.len()))];
                let (opts, context) = (self.opts, self.context);
                let prefix_searcher = self
                    .prefix_searcher
                    .get_or_insert_with(|| build_searcher(opts, context, BinaryDetection::none()));
                let mut prefix_sink =
                    CollectSink::new(self.matcher, work, (self.stop, self.held), self.terms);
                prefix_sink.cancelled = self.cancelled;
                prefix_sink.written = written;
                prefix_sink.source = Some(path);
                prefix_sink.line_scope = self.opts.line_scope.as_ref().map(|scope| (scope, path));
                let filled_prefix = filled
                    .as_deref()
                    .map(|filled| &filled[..prefix.len().min(filled.len())]);
                if filled_prefix.is_some() {
                    prefix_sink.inverted_over = Some(prefix);
                }
                prefix_searcher.search_slice(
                    self.matcher,
                    filled_prefix.unwrap_or(prefix),
                    &mut prefix_sink,
                )?;
                let mut outcome = FileOutcome::from(prefix_sink);
                outcome.binary = true;
                outcome.opaque = is_leading_binary(prefix, outcome.matched_lines > 0);
                outcome
            }
        };
        if self.copy {
            let kept =
                usize::try_from(FILE_TERMS_MAX_BYTES).map_or(bytes.len(), |at| at.min(bytes.len()));
            let eof = kept == bytes.len();
            outcome.copied = Some(CopiedRead {
                bytes: bytes[..kept].to_vec(),
                eof,
                written: written.to_vec(),
            });
        }
        Ok(outcome)
    }
}
