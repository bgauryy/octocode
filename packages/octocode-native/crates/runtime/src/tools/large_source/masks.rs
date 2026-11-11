//! What a read shows of a source that names a Kubernetes Secret or a JSON Web Key,
//! when no window of it can show what makes a value secret.
//!
//! A pass over every byte says whether the source names one ([`SourceNames`]), and
//! a walk of it says which bytes are secret. Those spans come after the pass that
//! holds the windows (`data` comes before `kind`), so each becomes a placeholder
//! edit ([`PlaceholderEdits`]) beside the scanner's ([`super::sealed`]): every
//! window shows the same sealed view. When the walk outgrew its budget, every
//! value of what the source names is hidden instead (`EveryValueScan`), as edits
//! of the same one view ([`super::edit_map`]), so a byte page of any line
//! continues in it.
use super::stream::seek;
use crate::execution::cancel::CancellationCheck;
use octocode_engine::security::{Named, SecretSpan, SecretSpans, SourceWalk};
use std::io::Read;

/// Source bytes the walk takes at once.
const READ_BYTES: usize = 64 * 1024;

/// What a source's windows must not show.
pub(super) enum Judged {
    /// These bytes, in order: all that is secret.
    Exact(Vec<SecretSpan>),
    /// A walk could not keep the source's structure, so no value in it can be told
    /// from another: every value of what it names is hidden, and where they are is
    /// kept as checkpoints ([`super::edit_map`]), never as a list.
    EveryValue,
}

impl Judged {
    /// About how many bytes this holds.
    pub(super) fn weight(&self) -> usize {
        match self {
            Self::Exact(spans) => spans.capacity() * std::mem::size_of::<SecretSpan>(),
            Self::EveryValue => 0,
        }
    }
}

/// Walk the source behind `file`, whose bytes a pass proved to be `digest`. Bytes
/// that no longer are fail the read: its windows came from the pass.
pub(super) fn judge(
    file: &std::fs::File,
    named: Named,
    digest: &str,
    budget: usize,
    cancel: &impl CancellationCheck,
) -> Result<Judged, String> {
    if !named.any() {
        return Ok(Judged::Exact(Vec::new()));
    }
    seek(file, 0)?;
    let mut walk = SourceWalk::new(named, budget);
    let mut hasher = octocode_engine::digest::HashWriter::default();
    let mut buffer = vec![0u8; READ_BYTES];
    loop {
        cancel.check()?;
        let read = match (&mut &*file).read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.to_string()),
        };
        super::count_read(read);
        hasher.update(&buffer[..read]);
        walk.feed(&buffer[..read]);
    }
    if hex::encode(hasher.finalize()) != digest {
        return Err("the source changed while it was read; read it again".to_owned());
    }
    Ok(match walk.finish() {
        SecretSpans::Exact(spans) => Judged::Exact(spans),
        SecretSpans::TooMany => Judged::EveryValue,
    })
}

/// The placeholders a secret span shows: one per line, from the line's first
/// byte that is not a blank to its end. Fed the span's bytes in chunks, it keeps only the current line's
/// first non-blank offset, so a span of any length is read through a bounded
/// buffer.
pub(super) struct PlaceholderEdits {
    text: Box<str>,
    base: usize,
    at: usize,
    first: Option<usize>,
}

impl PlaceholderEdits {
    pub(super) fn new(span: &SecretSpan) -> Self {
        Self {
            text: octocode_engine::security::redaction_placeholder(span.form.pattern()).into(),
            base: span.range.start,
            at: 0,
            first: None,
        }
    }

    /// The span's next bytes: the edits of the lines they end go to `out`, which
    /// the caller holds against the memory budget (a chunk of `n` bytes ends at
    /// most `n / 2 + 1` lines).
    pub(super) fn feed(&mut self, chunk: &[u8], out: &mut Vec<super::sealed::Edit>) {
        for &byte in chunk {
            if byte == b'\n' {
                self.close(out);
            } else if self.first.is_none() && !matches!(byte, b' ' | b'\t' | b'\r') {
                self.first = Some(self.at);
            }
            self.at += 1;
        }
    }

    /// The line ending at `self.at` (its `\n`, or the span's end) is one edit
    /// from its first non-blank byte; a blank line hides nothing.
    fn close(&mut self, out: &mut Vec<super::sealed::Edit>) {
        if let Some(first) = self.first.take() {
            out.push(super::sealed::Edit {
                range: self.base + first..self.base + self.at,
                text: self.text.clone(),
            });
        }
    }

    /// The last line's edit (at most one), once the whole span was fed.
    pub(super) fn finish(mut self, out: &mut Vec<super::sealed::Edit>) {
        self.close(out);
    }
}
