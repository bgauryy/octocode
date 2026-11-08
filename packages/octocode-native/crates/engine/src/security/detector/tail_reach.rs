//! Where a search of a comment line's keys can match (B165).
//!
//! A pattern read from each key of a comment line to the line's end costs the
//! rest of the line from every key, which is quadratic when the keys run into
//! each other. A match of the pattern from a key exists or not whatever the
//! keys before it were, so the answer is read off the pattern's DFA: a walk
//! from one key meets the walks of the keys before it in the same state at the
//! same place, and from there on has the same end. Each walk stops at the first
//! place and state an earlier walk of the line stood in.

use regex_automata::Anchored;
use regex_automata::hybrid::LazyStateID;
use regex_automata::hybrid::dfa::{Cache, DFA};
use regex_automata::util::start;
use std::collections::HashMap;

/// The walks of one pattern's DFA over the comment lines of a text.
pub(super) struct Reach {
    dfa: &'static DFA,
    cache: Cache,
    /// Whether a match follows, for each place and state a walk of the line
    /// stood in.
    memo: HashMap<(usize, LazyStateID), bool>,
    /// The cache's clear count when `memo`'s states were named: a cleared cache
    /// names its states anew.
    clears: usize,
    /// The bytes the DFA stepped over.
    steps: usize,
}

impl Reach {
    pub(super) fn new(dfa: &'static DFA) -> Self {
        let cache = dfa.create_cache();
        let clears = cache.clear_count();
        Self {
            dfa,
            cache,
            memo: HashMap::new(),
            clears,
            steps: 0,
        }
    }

    /// The bytes the DFA stepped over so far: each place of a line is stepped
    /// once for each state a walk reaches it in, and the states of a pattern
    /// at one place are few.
    pub(super) fn steps(&self) -> usize {
        self.steps
    }

    /// Starts the next line: its places are no place of an earlier line.
    pub(super) fn begin_line(&mut self) {
        self.memo.clear();
    }

    /// Whether the pattern, anchored at `from`, matches the line `text[from..to]`
    /// read as a line of its own. A fault of the cache or a state the DFA gave
    /// up on answers `true`: the caller then reads the line itself.
    ///
    /// A cleared cache names its states anew, so a state's name means one
    /// state only between two clears: the memo and the walk's own path hold
    /// names of the current one only ([`Reach::forget_if_cleared`]).
    pub(super) fn matches(&mut self, text: &[u8], from: usize, to: usize) -> bool {
        let mut path = Vec::new();
        self.forget_if_cleared(&mut path);
        let config = start::Config::new().anchored(Anchored::Yes);
        let Ok(mut state) = self.dfa.start_state(&mut self.cache, &config) else {
            return true;
        };
        self.forget_if_cleared(&mut path);
        let mut at = from;
        let found = loop {
            if state.is_match() || state.is_quit() {
                break true;
            }
            if state.is_dead() {
                break false;
            }
            if let Some(&known) = self.memo.get(&(at, state)) {
                break known;
            }
            if at == to {
                break self
                    .dfa
                    .next_eoi_state(&mut self.cache, state)
                    .map_or(true, |last| last.is_match());
            }
            path.push((at, state));
            self.steps += 1;
            match self.dfa.next_state(&mut self.cache, state, text[at]) {
                Ok(next) => state = next,
                Err(_) => break true,
            }
            // `state` is named by the cache as it is now; what came before a
            // clear is no name of it.
            self.forget_if_cleared(&mut path);
            at += 1;
        };
        self.forget_if_cleared(&mut path);
        self.memo
            .extend(path.into_iter().map(|place| (place, found)));
        found
    }

    /// How often the cache was cleared.
    #[cfg(test)]
    pub(super) fn clear_count(&self) -> usize {
        self.cache.clear_count()
    }

    /// Drops every state name the cache gave before its last clear: the
    /// memo's and those of `path`.
    fn forget_if_cleared(&mut self, path: &mut Vec<(usize, LazyStateID)>) {
        if self.cache.clear_count() != self.clears {
            self.memo.clear();
            path.clear();
            self.clears = self.cache.clear_count();
        }
    }
}

#[cfg(test)]
mod tests;
