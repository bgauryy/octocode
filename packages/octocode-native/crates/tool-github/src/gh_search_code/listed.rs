//! What a ghSearchCode walk carries between pages in the continuation's private `listed` field.
use octocode_engine::num::{saturating_u64, saturating_usize};
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
/// - for a page a response window showed only part of, [`PART`] and then
///   the files of the page its earlier parts showed: the next part reads the
///   same page and lists the rest ([`Listed::state_within`]).
///
/// The state grows by the files a page lists, so it is bounded by the
/// search window (each search's 1,000 reachable results) that also ends the
/// walk.
pub(super) struct Listed {
    totals: Option<Vec<usize>>,
    seen: HashSet<u64>,
    /// The files the walk's earlier pages listed, as the state carried them.
    earlier: String,
    /// The files this page lists, in order.
    listed: String,
    /// The files of this page that earlier parts of it showed.
    shown: HashSet<u64>,
    /// Whether the state carried [`PART`]: this call shows a later part of
    /// a page an earlier call began.
    part: bool,
}

/// Separates the files of earlier pages from the files earlier parts of
/// this page showed. No file digest takes it ([`digest`]).
const PART: u64 = u64::MAX;

/// How a page's file stands in the walk ([`Listed::insert`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Seen {
    /// Not listed before.
    New,
    /// An earlier part of this page showed it.
    Shown,
    /// An earlier page (or this one) already listed it.
    Repeat,
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
        let totals = (searches > 1 && numbers.len() >= searches)
            .then(|| numbers.drain(..searches).map(saturating_usize).collect());
        let at = numbers.iter().position(|&key| key == PART);
        let shown = match at {
            Some(at) => numbers.split_off(at).into_iter().skip(1).collect(),
            None => HashSet::new(),
        };
        let earlier: String = numbers.iter().map(|key| format!("{key:016x}")).collect();
        Self {
            totals,
            seen: numbers.into_iter().collect(),
            earlier,
            listed: String::new(),
            shown,
            part: at.is_some(),
        }
    }

    /// Whether this call shows a later part of a page an earlier call
    /// began: what the page states once (its shift) was stated there.
    pub(super) fn continues_page(&self) -> bool {
        self.part
    }

    /// How many files the walk's earlier pages listed: where this page
    /// starts in the walk.
    pub(super) fn earlier_count(&self) -> usize {
        self.earlier.len() / 16
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

    /// Record `item` and say how it stands in the walk: a file an earlier
    /// part of this page showed is this page's again, not a repeat.
    pub(super) fn insert(&mut self, item: &octocode_github::CodeSearchItem) -> Seen {
        let key = digest(&item.repository.full_name, &item.path);
        if !self.seen.insert(key) {
            return Seen::Repeat;
        }
        self.listed.push_str(&format!("{key:016x}"));
        if self.shown.contains(&key) {
            Seen::Shown
        } else {
            Seen::New
        }
    }

    /// Whether an earlier part of this page showed the file at `path` in
    /// `full_name` (`owner/repo`).
    pub(super) fn was_shown(&self, full_name: &str, path: &str) -> bool {
        self.shown.contains(&digest(full_name, path))
    }

    /// The state the next page's continuation carries, every file of this
    /// page listed; `None` while empty.
    pub(super) fn state(&self) -> Option<String> {
        let state = self.totals_state() + &self.earlier + &self.listed;
        (!state.is_empty()).then_some(state)
    }

    /// The state a continuation of this same page carries once `shown` (each
    /// `(owner, repo, path)`) are shown as well: the earlier pages' files,
    /// then [`PART`] and every file a part of this page showed.
    pub(super) fn state_within<'a>(
        &self,
        shown: impl IntoIterator<Item = (&'a str, &'a str, &'a str)>,
    ) -> String {
        let mut keys = self.shown.iter().copied().collect::<Vec<_>>();
        keys.extend(
            shown
                .into_iter()
                .map(|(owner, repo, path)| digest(&format!("{owner}/{repo}"), path)),
        );
        keys.sort_unstable();
        keys.dedup();
        let mut state = self.totals_state() + &self.earlier;
        state.push_str(&format!("{PART:016x}"));
        for key in keys {
            state.push_str(&format!("{key:016x}"));
        }
        state
    }

    fn totals_state(&self) -> String {
        self.totals
            .iter()
            .flatten()
            .map(|&total| format!("{:016x}", saturating_u64(total)))
            .collect()
    }
}

/// A file's identity across pages: its repository (`owner/repo`, any case)
/// and path (its blob `sha` changes with the index state, so it is not part
/// of it), as the first 8 bytes of their SHA-256; never [`PART`].
fn digest(full_name: &str, path: &str) -> u64 {
    let mut hash = octocode_engine::digest::HashWriter::default();
    hash.update(full_name.to_lowercase().as_bytes());
    hash.update([0]);
    hash.update(path.as_bytes());
    let bytes: [u8; 8] = hash.finalize()[..8].try_into().unwrap_or_default();
    u64::from_be_bytes(bytes).min(PART - 1)
}

#[cfg(test)]
mod tests;
