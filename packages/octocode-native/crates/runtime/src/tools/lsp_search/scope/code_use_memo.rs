//! Whether a file uses a name in code, kept across requests, so a repeat over
//! an unchanged file need not parse it again.
//!
//! An entry holds one answer under the [`SourceStamp`] of the bytes it came
//! from. Only a settled file is kept, so any later write moves its stamp, and
//! a stamp that differs is read again. No entry holds text, and none reaches
//! disk.
use super::Question;
use crate::tools::page_memo::PageMemo;
use octocode_engine::stamp::{SourceStamp, cache_stamp, settle_clock};
use std::path::Path;

struct Kept {
    stamp: SourceStamp,
    uses: bool,
}

/// Answers by file, name and question, against the one process budget.
static USES: PageMemo<Kept> = PageMemo::new();

/// The key already says what an answer depends on.
const ANY_POLICY: &str = "";

/// The kept answer for `file` while its stamp holds, else `read`'s, kept when
/// the file was settled before the read and unchanged after it. `read` is
/// `None` when it could not read the file, which is not kept.
pub(super) fn kept_or_read(
    symbol: &str,
    file: &str,
    question: Question,
    read: impl FnOnce() -> Option<bool>,
) -> Option<bool> {
    let clock = settle_clock();
    let stat = || {
        std::fs::metadata(Path::new(file))
            .ok()
            .filter(std::fs::Metadata::is_file)
    };
    let meta = stat();
    let stamp = meta.as_ref().and_then(cache_stamp);
    let key = format!("{file}\0{question:?}\0{symbol}");
    if let Some(stamp) = stamp {
        let kept = USES.get(&key, ANY_POLICY, |kept| {
            (kept.stamp == stamp).then_some(kept.uses)
        });
        match kept {
            Some(Some(uses)) => return Some(uses),
            Some(None) => USES.evict(&key),
            None => {}
        }
    }
    #[cfg(test)]
    READS.note(Path::new(file));
    let uses = read()?;
    let settled = meta
        .as_ref()
        .is_some_and(|meta| SourceStamp::settled(meta, clock).is_some())
        && stat().as_ref().and_then(cache_stamp) == stamp;
    if let (true, Some(stamp)) = (settled, stamp) {
        let bytes = std::mem::size_of::<Kept>();
        USES.put(key, ANY_POLICY.to_owned(), bytes, Kept { stamp, uses });
    }
    Some(uses)
}

/// Test-only count of the times each file was read for its answer.
#[cfg(test)]
pub(super) static READS: super::path_counter::PathCounter = super::path_counter::PathCounter::new();
