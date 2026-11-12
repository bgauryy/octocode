//! A file with a line over the line buffer's bound, searched from its bytes held
//! whole once, so that line is searched like any other.
use super::{
    BinaryDetection, CollectSink, CopiedRead, FILE_TERMS_MAX_BYTES, FileOutcome, FileSearcher,
    HEAD_BYTES, MAX_BINARY_PREFIX_BYTES, MatchWork, build_searcher, is_leading_binary, read_prefix,
    relevance,
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
        let bytes = read_prefix(file, file.metadata()?.len())?;
        self.search_bytes(&bytes, path, work)
    }

    /// [`Self::search`] of a file's text held whole: its bytes, or the text
    /// its judge serves ([`super::FileSearcher::judged`]).
    pub(super) fn search_bytes(
        &mut self,
        bytes: &[u8],
        path: &Path,
        work: MatchWork,
    ) -> std::io::Result<FileOutcome> {
        let mut sink = CollectSink::new(
            self.matcher,
            work,
            (self.stop, self.held),
            (self.words, self.terms),
        );
        sink.test_code = self.opts.test_code.as_ref().map(|scope| (scope, path));
        sink.source = Some(path);
        sink.inspect_deferred = self
            .opts
            .inspect_deferred
            .as_ref()
            .map(|inspect| (inspect, path));
        self.searcher.search_slice(self.matcher, bytes, &mut sink)?;
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
                let mut prefix_sink = CollectSink::new(
                    self.matcher,
                    work,
                    (self.stop, self.held),
                    (self.words, self.terms),
                );
                prefix_sink.source = Some(path);
                prefix_searcher.search_slice(self.matcher, prefix, &mut prefix_sink)?;
                let mut outcome = FileOutcome::from(prefix_sink);
                outcome.binary = true;
                outcome.opaque = is_leading_binary(prefix, outcome.matched_lines > 0);
                outcome
            }
        };
        if self.keep_head && outcome.matched_lines > 0 {
            outcome.generated_header = Some(relevance::has_generated_header(
                &bytes[..bytes.len().min(HEAD_BYTES)],
            ));
        }
        if self.copy {
            let kept =
                usize::try_from(FILE_TERMS_MAX_BYTES).map_or(bytes.len(), |at| at.min(bytes.len()));
            let eof = kept == bytes.len();
            outcome.copied = Some(CopiedRead {
                bytes: bytes[..kept].to_vec(),
                eof,
            });
        }
        Ok(outcome)
    }
}
