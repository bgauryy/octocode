//! A row's request cap: the provider requests its evaluations may send,
//! counted at each send. A row states its ceiling (every request a refusal
//! or a missing answer can add, `at_most`, plus the transient retries the row
//! shares, `room`) and is held to it, capped or not: a capped row is admitted
//! only when that ceiling fits its `maxRequests`. Only the room past
//! `at_most` is taken by a transient failure's retry, so a retry never takes
//! a request the row's own plan may still need, and nothing is sent once the
//! cap is spent.
use super::ClassificationError;
use octocode_contract_types::ErrorCode;
use std::sync::{Mutex, PoisonError};

/// What a transient failure says when the row's shared retries were spent
/// before its request could be retried as often as the runtime retries one.
pub(crate) const RETRIES_SPENT: &str = "This row's shared transient retries were spent before this request could be retried; its units are left unanswered for a later retry.";

/// A send the cap has no room for: it is not sent.
pub(crate) fn spent() -> ClassificationError {
    ClassificationError::new(
        ErrorCode::ClassificationOverBudget,
        "This row's maxRequests is spent; the request was not sent.",
        "Raise maxRequests to the row's usage.requestsCeiling, or narrow the source.",
    )
}

/// What a row may still send.
pub struct RequestCap {
    left: Mutex<Left>,
    /// The transient retries the row shares: what its stated ceiling counts
    /// past `at_most`.
    room: u64,
    /// A row without `maxRequests`: its ceiling rests on the provider's
    /// stated windows, so a request the provider refuses although its window
    /// holds it still gets its refusal's retry, past the ceiling and counted
    /// ([`Self::past`]); transient retries never pass the room.
    soft: bool,
}

#[derive(Clone, Copy)]
struct Left {
    /// Requests the row may still send, retries included.
    sends: u64,
    /// Of `sends`, those past the row's ceiling: the room retries take.
    retries: u64,
    /// Requests sent past the ceiling ([`RequestCap::soft`]).
    past: u64,
}

/// What one send took from the cap ([`RequestCap::take`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Grant {
    /// The transient retries the send may take.
    pub(crate) retries: u32,
    /// The send is past the ceiling ([`RequestCap::soft`]).
    past: bool,
}

impl RequestCap {
    /// The cap of a row whose requests are at most `at_most` without
    /// transient retries, and which shares `room` of them: its stated
    /// ceiling, `at_most + room` ([`Self::ceiling`]).
    pub fn held(at_most: u64, room: u64) -> Self {
        Self::new(at_most, room, false)
    }

    /// [`Self::held`] for a row without `maxRequests` ([`Self::soft`]).
    pub fn implicit(at_most: u64, room: u64) -> Self {
        Self::new(at_most, room, true)
    }

    fn new(at_most: u64, room: u64, soft: bool) -> Self {
        let sends = Self::ceiling(at_most, room);
        Self {
            left: Mutex::new(Left {
                sends,
                retries: sends.saturating_sub(at_most),
                past: 0,
            }),
            room,
            soft,
        }
    }

    /// Requests sent past the ceiling: refusal retries of requests the
    /// provider's stated windows hold ([`Self::soft`]).
    pub fn past(&self) -> u64 {
        self.with(|left| left.past)
    }

    /// The ceiling a row of `at_most` requests sharing `room` transient
    /// retries states and is held to; a row that sends nothing has nothing
    /// to retry.
    pub fn ceiling(at_most: u64, room: u64) -> u64 {
        if at_most == 0 {
            0
        } else {
            at_most.saturating_add(room)
        }
    }

    /// The transient retries the row shares.
    pub fn room(&self) -> u64 {
        self.room
    }

    fn with<T>(&self, change: impl FnOnce(&mut Left) -> T) -> T {
        change(&mut self.left.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Takes one send and up to `retries` retries of it; `None` when the cap
    /// is spent, and the request is not sent. A send reserves only its share
    /// of the retry room left (the room over the planned sends left, rounded
    /// up), so requests in flight at once each keep a retry instead of the
    /// first holding them all. Returns the retries granted.
    pub(crate) fn take(&self, retries: u32) -> Option<Grant> {
        let soft = self.soft;
        self.with(|left| {
            if left.sends == 0 {
                if !soft {
                    return None;
                }
                left.past = left.past.saturating_add(1);
                return Some(Grant {
                    retries: 0,
                    past: true,
                });
            }
            // The planned sends left, this one included; a send past them
            // takes the retry room.
            let planned = left.sends - left.retries.min(left.sends);
            left.sends -= 1;
            if planned == 0 {
                left.retries = left.retries.saturating_sub(1);
            }
            let share = left.retries.div_ceil(planned.max(1));
            let granted = u64::from(retries).min(share).min(left.retries);
            left.retries -= granted;
            left.sends -= granted;
            Some(Grant {
                retries: u32::try_from(granted).unwrap_or(retries),
                past: false,
            })
        })
    }

    /// Returns what a send took and did not use: `attempts` is how many
    /// times it was sent (`0`: never, the gate or the deadline stopped it).
    pub(crate) fn settle(&self, grant: Grant, attempts: u64) {
        let unused_retries = u64::from(grant.retries).saturating_sub(attempts.saturating_sub(1));
        self.with(|left| {
            if grant.past {
                if attempts == 0 {
                    left.past = left.past.saturating_sub(1);
                }
                return;
            }
            left.retries = left.retries.saturating_add(unused_retries);
            left.sends = left
                .sends
                .saturating_add(unused_retries)
                .saturating_add(u64::from(attempts == 0));
        });
    }

    /// Requests the row may still send.
    pub fn left(&self) -> u64 {
        self.with(|left| left.sends)
    }
}

#[cfg(test)]
mod tests {
    use super::{Grant, RequestCap};

    /// What `take` granted, as retries.
    fn take(cap: &RequestCap, retries: u32) -> Option<u32> {
        cap.take(retries).map(|grant| grant.retries)
    }

    fn grant(retries: u32) -> Grant {
        Grant {
            retries,
            past: false,
        }
    }

    #[test]
    fn a_send_takes_one_and_retries_only_past_the_ceiling() {
        let cap = RequestCap::held(3, 1);
        assert_eq!(RequestCap::ceiling(3, 1), 4);
        assert_eq!(take(&cap, 2), Some(1), "one retry fits past the ceiling");
        cap.settle(grant(1), 1);
        assert_eq!(cap.left(), 3, "the unused retry comes back");
        assert_eq!(take(&cap, 2), Some(1));
        cap.settle(grant(1), 2);
        assert_eq!(cap.left(), 1, "a used retry stays spent");
        assert_eq!(take(&cap, 2), Some(0), "no room past the ceiling is left");
        cap.settle(grant(0), 1);
        assert_eq!(take(&cap, 2), None, "a spent cap sends nothing");
    }

    #[test]
    fn the_shared_retries_never_take_a_planned_send() {
        // Two planned requests sharing three retries: the first reserves its
        // share (two), spends them, and the second is still sent with the
        // one left.
        let cap = RequestCap::held(2, 3);
        assert_eq!(cap.room(), 3);
        assert_eq!(take(&cap, 3), Some(2));
        cap.settle(grant(2), 3);
        assert_eq!(cap.left(), 2, "the second planned send and one retry stay");
        assert_eq!(take(&cap, 3), Some(1), "the retry left is its");
        cap.settle(grant(1), 2);
        assert_eq!(take(&cap, 3), None, "the ceiling is spent");
        assert_eq!(
            RequestCap::ceiling(0, 3),
            0,
            "nothing to send, nothing to retry"
        );
    }

    #[test]
    fn requests_in_flight_at_once_each_keep_a_retry() {
        // Three requests start before any settles: each reserves one of the
        // three retries, so a failure of any one is retried.
        let cap = RequestCap::held(3, 3);
        assert_eq!(
            [take(&cap, 3), take(&cap, 3), take(&cap, 3)],
            [Some(1), Some(1), Some(1)]
        );
        // A retry a send did not use comes back for a later one.
        cap.settle(grant(1), 1);
        assert_eq!(cap.left(), 1);
        cap.settle(grant(1), 1);
        cap.settle(grant(1), 1);
        assert_eq!(cap.left(), 3, "every unused retry is back");
    }

    #[test]
    fn a_row_without_max_requests_still_sends_a_refusals_retry_counted() {
        // One planned request and no retry room: a refusal's long-window
        // retry the windows said could not happen is sent past the ceiling,
        // without transient retries, and counted; a capped row's is not.
        let implicit = RequestCap::implicit(1, 0);
        assert_eq!(take(&implicit, 3), Some(0));
        implicit.settle(grant(0), 1);
        let past = implicit.take(3).expect("sent past the ceiling");
        assert_eq!(past.retries, 0, "no transient retry past the room");
        implicit.settle(past, 1);
        assert_eq!(implicit.past(), 1);
        let capped = RequestCap::held(1, 0);
        assert_eq!(take(&capped, 3), Some(0));
        capped.settle(grant(0), 1);
        assert_eq!(take(&capped, 3), None, "maxRequests holds");
        assert_eq!(capped.past(), 0);
    }

    #[test]
    fn a_send_that_never_left_gives_everything_back() {
        let cap = RequestCap::held(1, 1);
        assert_eq!(take(&cap, 3), Some(1));
        cap.settle(grant(1), 0);
        assert_eq!(cap.left(), 2);
    }
}
