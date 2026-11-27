//! Each file's outcome kept across scans: a repeat of a scan, or the first
//! page of the same query again, reads only the files whose stamp moved.
//!
//! An outcome depends on the file's bytes, the parser that reads it, the
//! query, what the demand keeps (the caller's price is a function of the
//! match and the caller's key), the byte and parse limits and the judge. It
//! is kept under all of them with the file's stamp, read before its bytes
//! were, and only when that stamp was settled when the scan started
//! ([`SourceStamp::settled_by`]), so any later write moves the stamp. An
//! outcome the scan cut short (a deadline, a limit, an unreadable file) is not
//! kept, nor materialized matches. Eviction only costs a read and a parse.

use super::file_scan::Tally;
use crate::stamp::SourceStamp;
use crate::structural::types::{FileMatches, StructuralDemand, StructuralSearchDetailedFileResult};
use octocode_io::memo::Memo;

/// Every scan's kept outcomes, by [`Kept::key`].
static OUTCOMES: Memo<String, Outcome> = Memo::new(&crate::memory::MEMORY);

/// One file's kept outcome and the stamp it was read at.
struct Outcome {
    stamp: SourceStamp,
    tally: Tally,
    file: StructuralSearchDetailedFileResult,
}

/// One scan's view of the kept outcomes: the key its files are kept under,
/// and when it started.
pub(in crate::structural) struct Kept {
    prefix: String,
    settled_ns: u64,
}

/// What a scan's outcomes depend on besides each file: its query, its byte
/// and parse limits, and whether a judge reads its sources.
pub(in crate::structural) struct Inputs<'a> {
    pub(in crate::structural) pattern: Option<&'a str>,
    pub(in crate::structural) rule: Option<&'a serde_json::Value>,
    pub(in crate::structural) max_file_bytes: u64,
    pub(in crate::structural) parse_budget: usize,
    pub(in crate::structural) judged: bool,
}

impl Kept {
    /// The kept outcomes of a scan under the caller's key `caller` (its
    /// price's inputs), keeping what `demand` keeps; `None` when the demand
    /// materializes matches, which are not kept.
    pub(in crate::structural) fn new(
        caller: &str,
        demand: &StructuralDemand<'_>,
        inputs: &Inputs<'_>,
    ) -> Option<Self> {
        let demand = match demand {
            StructuralDemand::Matches => return None,
            StructuralDemand::Count | StructuralDemand::Exists => "count",
            StructuralDemand::Priced(_) | StructuralDemand::Paged(..) => "priced",
        };
        let query = serde_json::to_string(&(inputs.pattern, inputs.rule)).ok()?;
        let query = crate::digest::sha256(query.as_bytes());
        let settled_ns = crate::stamp::settle_clock()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|since| u64::try_from(since.as_nanos()).ok())?;
        Some(Self {
            prefix: format!(
                "{}:{caller}\0{demand}\0{query}\0{}\0{}\0{}",
                caller.len(),
                inputs.max_file_bytes,
                inputs.parse_budget,
                inputs.judged
            ),
            settled_ns,
        })
    }

    /// The key of `path` read as `ext` (each part's length first, so no two
    /// keys share a spelling).
    fn key(&self, ext: &str, path: &str) -> String {
        format!("{}\0{}:{ext}\0{path}", self.prefix, ext.len())
    }

    /// The outcome kept for `path` read as `ext`, while its stamp is still
    /// `stamp`.
    pub(in crate::structural) fn get(
        &self,
        path: &str,
        ext: &str,
        stamp: SourceStamp,
    ) -> Option<(Tally, StructuralSearchDetailedFileResult)> {
        let hit = OUTCOMES.get(&self.key(ext, path))?;
        let kept = hit.value.as_ref();
        if kept.stamp != stamp {
            return None;
        }
        Some((kept.tally, copied(&kept.file)?))
    }

    /// Keep `file`'s outcome, read as `ext` at `stamp`, when the stamp was
    /// settled when the scan started and the outcome is whole: a file the
    /// prefilter skipped, or one parsed and matched to its end.
    pub(in crate::structural) fn keep(
        &self,
        ext: &str,
        stamp: SourceStamp,
        tally: &Tally,
        file: &StructuralSearchDetailedFileResult,
    ) {
        let whole = matches!(
            (tally, file.status.as_str()),
            (Tally::PreFilter, "skippedByPreFilter") | (Tally::Parsed(_), "ok")
        );
        if !whole || !stamp.settled_by(self.settled_ns) {
            return;
        }
        let Some(copy) = copied(file) else {
            return;
        };
        let key = self.key(ext, &file.path);
        let weight = 128
            + key.len()
            + match &copy.matches {
                FileMatches::Priced(prices) => prices.len() * 4,
                FileMatches::Held(_) | FileMatches::Counted(_) => 0,
            }
            + copy.source_sha256.as_ref().map_or(0, String::len)
            + copy
                .diagnostics
                .iter()
                .map(|d| 64 + d.message.len() + d.path.as_ref().map_or(0, String::len))
                .sum::<usize>()
            + copy
                .recovered_bodies
                .iter()
                .map(|body| 32 + body.name.len())
                .sum::<usize>();
        OUTCOMES.insert(
            key,
            Outcome {
                stamp,
                tally: *tally,
                file: copy,
            },
            weight,
            None,
        );
    }
}

/// A copy of `file` as the scan returns it; `None` when it holds
/// materialized matches.
fn copied(file: &StructuralSearchDetailedFileResult) -> Option<StructuralSearchDetailedFileResult> {
    let matches = match &file.matches {
        FileMatches::Priced(prices) => FileMatches::Priced(prices.clone()),
        FileMatches::Counted(count) => FileMatches::Counted(*count),
        FileMatches::Held(held) if held.is_empty() => FileMatches::Held(Vec::new()),
        FileMatches::Held(_) => return None,
    };
    Some(StructuralSearchDetailedFileResult {
        path: file.path.clone(),
        source_sha256: file.source_sha256.clone(),
        status: file.status.clone(),
        language_id: file.language_id.clone(),
        skipped_reason: file.skipped_reason.clone(),
        matches,
        recovered: file.recovered,
        recovered_bodies: file.recovered_bodies.clone(),
        diagnostics: file.diagnostics.clone(),
    })
}
