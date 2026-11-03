//! What a ghSearchCode walk carries between pages in the continuation's private `listed` field.
use std::collections::HashSet;

/// The walk state of an issued continuation (the private `listed` field):
///
/// - the files the walk has listed: GitHub's index moves between page reads,
///   so a file can come back on a later page, and only the walk's own saved
///   state records every earlier page. Each file is a 16-hex digest of its
///   identity ([`digest`]);
/// - for a merged multi-`extensions` search, each search's last stated
///   match count, so a later page lays itself out without re-reading page 1
///   of every search. They lead the field, one 16-hex number per search in
///   extension order, followed by the digests.
///
/// The state grows by the files a page lists, so it is bounded by the
/// search window (each search's 1,000 reachable results) that also ends the
/// walk.
pub(super) struct Listed {
    totals: Option<Vec<usize>>,
    seen: HashSet<u64>,
    digests: String,
}

impl Listed {
    /// The walk state a continuation carried; empty on a walk's first page.
    /// `searches` is the number of merged searches (1 when not merged).
    /// The contract admits only whole 16-hex numbers.
    pub(super) fn of(encoded: Option<&str>, searches: usize) -> Self {
        let mut numbers = encoded
            .unwrap_or_default()
            .as_bytes()
            .chunks(16)
            .filter_map(|chunk| u64::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok())
            .collect::<Vec<_>>();
        let totals = (searches > 1 && numbers.len() >= searches).then(|| {
            numbers
                .drain(..searches)
                .map(|total| usize::try_from(total).unwrap_or(usize::MAX))
                .collect()
        });
        let digests = numbers.iter().map(|key| format!("{key:016x}")).collect();
        Self {
            totals,
            seen: numbers.into_iter().collect(),
            digests,
        }
    }

    /// Each merged search's match count as the walk last stated it; `None`
    /// when the walk carries none (a first page, or a page asked for directly).
    pub(super) fn totals(&self) -> Option<&[usize]> {
        self.totals.as_deref()
    }

    /// Keep the match counts the next page lays itself out from.
    pub(super) fn set_totals(&mut self, totals: Vec<usize>) {
        self.totals = Some(totals);
    }

    /// Record `item`; `false` when the walk already listed it.
    pub(super) fn insert(&mut self, item: &octocode_github::CodeSearchItem) -> bool {
        let key = digest(item);
        let new = self.seen.insert(key);
        if new {
            self.digests.push_str(&format!("{key:016x}"));
        }
        new
    }

    /// The state the next page's continuation carries; `None` while empty.
    pub(super) fn state(&self) -> Option<String> {
        let totals = self
            .totals
            .iter()
            .flatten()
            .map(|&total| format!("{:016x}", u64::try_from(total).unwrap_or(u64::MAX)))
            .collect::<String>();
        let state = totals + &self.digests;
        (!state.is_empty()).then_some(state)
    }
}

/// A file's identity across pages: its repository and path (its blob `sha`
/// changes with the index state, so it is not part of it), as the first 8
/// bytes of their SHA-256.
fn digest(item: &octocode_github::CodeSearchItem) -> u64 {
    let mut hash = octocode_engine::digest::HashWriter::default();
    hash.update(item.repository.full_name.to_lowercase().as_bytes());
    hash.update([0]);
    hash.update(item.path.as_bytes());
    let bytes: [u8; 8] = hash.finalize()[..8].try_into().unwrap_or_default();
    u64::from_be_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totals_lead_the_state_and_digests_follow() {
        let carried = format!("{:016x}{:016x}{:016x}", 7, 9, 0xabcd);
        let listed = Listed::of(Some(&carried), 2);
        assert_eq!(listed.totals(), Some(&[7, 9][..]));
        assert_eq!(listed.state().as_deref(), Some(carried.as_str()));
        assert!(Listed::of(Some(&carried), 1).totals().is_none());
        assert!(Listed::of(None, 2).totals().is_none());
        assert!(Listed::of(None, 2).state().is_none());
    }
}
