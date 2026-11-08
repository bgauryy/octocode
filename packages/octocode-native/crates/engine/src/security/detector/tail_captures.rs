//! The captures of a pattern read from many keys of one comment line (B165).
//!
//! A read of a key's value to the line's end ([`super::tail_reads`]) costs the
//! rest of the line from each key, so a line of keys whose values the test
//! rejects (`` token=`a `` repeated) costs the square of its length. The reads
//! share their ends: a leftmost-first simulation of the pattern's NFA (a
//! PikeVM) from each key meets the walks of the keys before it.
//!
//! A walk holds, at each place, the ordered list of the NFA states its threads
//! stand in (after the epsilon closure at that place). What happens from there
//! on depends on that list alone: the bytes after the place, and the
//! look-arounds at later places, are the same for every walk of the line (they
//! all end at the line's end, and only the walk's own first place reads as a
//! haystack start). So two walks that reach one place with the same list have
//! the same future: the same thread of the list (by index) wins, and every
//! capture slot the winner writes after the place is written at the same
//! position. The memo holds, for each place and list a walk stepped, that
//! winner's index and the slots it writes after the place, or that no thread
//! of the list ever matches. A walk that meets a memo entry stops: its match
//! is the entry's slots over those its own thread at that index carried in, or,
//! for an entry with no match, the last match the walk recorded before. That is
//! exact; nothing is approximated.
//!
//! Each place and list is stepped once per line, so a line costs its length
//! times the number of distinct lists a place is reached in, which the pattern
//! bounds, as the DFA's states bound [`super::tail_reach::Reach`].

use regex_automata::nfa::thompson::{NFA, State};
use regex_automata::util::primitives::StateID;
use std::collections::HashMap;

/// A thread of a walk at one place: its NFA state, the index of the thread
/// of the place before it came from, and the capture slots the epsilon
/// closure at this place wrote on its way.
struct Thread {
    state: StateID,
    parent: usize,
    writes: Box<[usize]>,
}

/// What the future of a place's list is: the winning thread's index and the
/// `(slot, position)` writes it makes after the place, or no match.
type Future = Option<(usize, Vec<(usize, usize)>)>;

/// A list a walk stepped, by its states, with its future.
type Entry = (Box<[StateID]>, Future);

/// A place of a walk and the threads it stood in there.
type Place = (usize, Vec<Thread>);

/// How a walk ended.
enum End {
    /// No thread is left, or the line's end was read.
    Done,
    /// The list at the current place is in the memo.
    Memo(Future),
}

/// The capture slots of a match, by slot index: absolute text positions.
pub(super) type Slots = Vec<Option<usize>>;

/// The memoized walks of one pattern's NFA over the comment lines of a text.
pub(super) struct TailCaptures {
    nfa: &'static NFA,
    /// Each place's lists a walk of the line stepped, with their futures.
    memo: HashMap<usize, Vec<Entry>>,
    /// The closure's mark of each NFA state, by the stamp of the list it was
    /// added to; empty until the first walk.
    seen: Vec<usize>,
    stamp: usize,
    /// The places stepped.
    steps: usize,
}

impl TailCaptures {
    pub(super) fn new(nfa: &'static NFA) -> Self {
        Self {
            nfa,
            memo: HashMap::new(),
            seen: Vec::new(),
            stamp: 0,
            steps: 0,
        }
    }

    /// Starts the next line: its places and end are no earlier line's.
    pub(super) fn begin_line(&mut self) {
        self.memo.clear();
    }

    /// The places stepped so far: each place of a line once per distinct
    /// list a walk reached it in.
    pub(super) fn steps(&self) -> usize {
        self.steps
    }

    /// The slots of the leftmost-first match of the pattern anchored at
    /// `from` in `text[from..to]` read as a haystack of its own, as the regex
    /// crate reports them; `None` when it does not match there.
    pub(super) fn find(&mut self, text: &[u8], from: usize, to: usize) -> Option<Slots> {
        if self.seen.is_empty() {
            self.seen = vec![0; self.nfa.states().len()];
        }
        let window = &text[from..to];
        let mut history: Vec<Place> = Vec::new();
        let mut list = Vec::new();
        self.stamp += 1;
        self.closure(
            self.nfa.start_anchored(),
            usize::MAX,
            from,
            window,
            from,
            &mut list,
        );
        let mut last_match = None;
        let mut at = from;
        let end = loop {
            if list.is_empty() {
                break End::Done;
            }
            if let Some(future) = self.recall(at, &list) {
                break End::Memo(future);
            }
            self.steps += 1;
            self.stamp += 1;
            let mut next = Vec::new();
            for (index, thread) in list.iter().enumerate() {
                if matches!(self.nfa.state(thread.state), State::Match { .. }) {
                    // Leftmost-first: the threads after a match are cut.
                    last_match = Some((history.len(), index));
                    break;
                }
                if at < to
                    && let Some(target) = step(self.nfa.state(thread.state), text[at])
                {
                    self.closure(target, index, at + 1, window, from, &mut next);
                }
            }
            // At the line's end nothing is stepped, so `next` is empty.
            history.push((at, std::mem::replace(&mut list, next)));
            if at == to {
                break End::Done;
            }
            at += 1;
        };
        self.settle(history, (at, list), end, last_match)
    }

    /// The future of `list` at `at` when a walk stepped it before.
    fn recall(&self, at: usize, list: &[Thread]) -> Option<Future> {
        self.memo.get(&at)?.iter().find_map(|(states, future)| {
            (states.len() == list.len()
                && states
                    .iter()
                    .zip(list)
                    .all(|(state, thread)| *state == thread.state))
            .then(|| future.clone())
        })
    }

    /// Records the future of every list of a walk and returns its match: the
    /// winner is the memo entry's thread where the walk met one with a match,
    /// else the last match the walk recorded; the slots are the writes along
    /// the winner's ancestry, the latest of each slot.
    fn settle(
        &mut self,
        mut history: Vec<Place>,
        (at, list): Place,
        end: End,
        last_match: Option<(usize, usize)>,
    ) -> Option<Slots> {
        let found = matches!(end, End::Memo(Some(_))) || last_match.is_some();
        let mut writes = Vec::new();
        // The place in `history` the winner's ancestry is read from, and the
        // index of its thread there; `history.len()` is the place met in
        // the memo, whose list is `list`.
        let (mut place, mut index) = match end {
            End::Memo(Some((winner, after))) => {
                writes = after;
                let thread = &list[winner];
                add_writes(&mut writes, &thread.writes, at);
                (history.len().checked_sub(1), thread.parent)
            }
            End::Memo(None) | End::Done => match last_match {
                Some((place, index)) => (Some(place), index),
                None => (None, 0),
            },
        };
        // Places after the winner's last one see no match from their list.
        let ancestry_end = place.map_or(0, |place| place + 1);
        for (at, list) in history.drain(ancestry_end..) {
            self.remember(at, &list, None);
        }
        while let Some(current) = place {
            let (at, list) = &history[current];
            self.remember(*at, list, Some((index, writes.clone())));
            let thread = &list[index];
            add_writes(&mut writes, &thread.writes, *at);
            index = thread.parent;
            place = current.checked_sub(1);
        }
        if !found {
            return None;
        }
        let mut slots = vec![None; self.nfa.group_info().slot_len()];
        for (slot, position) in writes {
            slots[slot] = Some(position);
        }
        Some(slots)
    }

    fn remember(&mut self, at: usize, list: &[Thread], future: Future) {
        let states = list.iter().map(|thread| thread.state).collect();
        self.memo.entry(at).or_default().push((states, future));
    }

    /// Adds to `list` the threads the epsilon closure of `start` at `at`
    /// reaches, in priority order, each state once per list. `window` starts
    /// at `from`: look-arounds read it as their haystack.
    fn closure(
        &mut self,
        start: StateID,
        parent: usize,
        at: usize,
        window: &[u8],
        from: usize,
        list: &mut Vec<Thread>,
    ) {
        enum Frame {
            Explore(StateID),
            Truncate(usize),
        }
        let mut stack = vec![Frame::Explore(start)];
        let mut writes = Vec::new();
        while let Some(frame) = stack.pop() {
            let state = match frame {
                Frame::Truncate(len) => {
                    #[cfg_attr(
                        octocode_clippy_ratchet,
                        expect(
                            clippy::disallowed_methods,
                            reason = "undoes this branch's capture writes when the closure backtracks; no evidence is cut"
                        )
                    )]
                    writes.truncate(len);
                    continue;
                }
                Frame::Explore(state) => state,
            };
            let mark = &mut self.seen[state.as_usize()];
            if *mark == self.stamp {
                continue;
            }
            *mark = self.stamp;
            match self.nfa.state(state) {
                State::ByteRange { .. }
                | State::Sparse(_)
                | State::Dense(_)
                | State::Match { .. } => {
                    list.push(Thread {
                        state,
                        parent,
                        writes: writes.clone().into_boxed_slice(),
                    });
                }
                State::Fail => {}
                State::Look { look, next } => {
                    if self.nfa.look_matcher().matches(*look, window, at - from) {
                        stack.push(Frame::Explore(*next));
                    }
                }
                State::Union { alternates } => {
                    stack.extend(alternates.iter().rev().map(|&alt| Frame::Explore(alt)));
                }
                State::BinaryUnion { alt1, alt2 } => {
                    stack.push(Frame::Explore(*alt2));
                    stack.push(Frame::Explore(*alt1));
                }
                State::Capture { next, slot, .. } => {
                    stack.push(Frame::Truncate(writes.len()));
                    writes.push(slot.as_usize());
                    stack.push(Frame::Explore(*next));
                }
            }
        }
    }
}

/// The span group `index` of a match holds, from its slots.
pub(super) fn group(slots: &Slots, index: usize) -> Option<std::ops::Range<usize>> {
    Some(slots.get(index * 2).copied()??..slots.get(index * 2 + 1).copied()??)
}

/// Adds the writes of one closure at `at` to `writes`, which holds those of
/// later places: a slot keeps its latest write.
fn add_writes(writes: &mut Vec<(usize, usize)>, closure: &[usize], at: usize) {
    for &slot in closure {
        if !writes.iter().any(|&(written, _)| written == slot) {
            writes.push((slot, at));
        }
    }
}

/// The state a byte leads to from `state`, if any.
fn step(state: &State, byte: u8) -> Option<StateID> {
    match state {
        State::ByteRange { trans } => trans.matches_byte(byte).then_some(trans.next),
        State::Sparse(sparse) => sparse.matches_byte(byte),
        State::Dense(dense) => dense.matches_byte(byte),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
