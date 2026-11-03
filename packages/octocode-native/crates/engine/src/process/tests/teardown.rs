//! Tree teardown against a scripted process table: a descendant's pid can be
//! recycled by an unrelated process at any point between the walk and a
//! signal, and that process must never be stopped or killed.

use crate::process::identity::{Process, ProcessTable};
use crate::process::teardown::tear_down;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

const ROOT: u32 = 100;
const ROOT_PID: libc::pid_t = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    Walk,
    Signal(libc::pid_t, libc::c_int),
}

/// Successive walks are replayed in order (the last repeats); `live` maps
/// each pid to the start time of whichever process holds it now. After the
/// `trigger` event, `pid` is recycled: the descendant is gone and an
/// unrelated process with a new start time holds its pid.
struct Scripted {
    walks: RefCell<VecDeque<Vec<Process>>>,
    live: RefCell<HashMap<u32, u64>>,
    log: RefCell<Vec<Event>>,
    recycle: Option<(Event, u32)>,
}

const STRANGER_START: u64 = 999;

impl Scripted {
    fn new(walks: Vec<Vec<Process>>, recycle: Option<(Event, u32)>) -> Self {
        let live = walks
            .iter()
            .flatten()
            .map(|process| (process.pid, process.start))
            .collect();
        Self {
            walks: RefCell::new(walks.into()),
            live: RefCell::new(live),
            log: RefCell::default(),
            recycle,
        }
    }

    fn record(&self, event: Event) {
        self.log.borrow_mut().push(event);
        if let Some((trigger, pid)) = self.recycle
            && trigger == event
        {
            self.live.borrow_mut().insert(pid, STRANGER_START);
        }
    }

    fn signals_to(&self, pid: u32) -> Vec<libc::c_int> {
        let pid = libc::pid_t::try_from(pid).expect("pid");
        self.log
            .borrow()
            .iter()
            .filter_map(|event| match *event {
                Event::Signal(target, signal) if target == pid => Some(signal),
                _ => None,
            })
            .collect()
    }
}

impl ProcessTable for Scripted {
    type Handle = u32;

    fn descendants(&self, _root: u32) -> Vec<Process> {
        let walk = {
            let mut walks = self.walks.borrow_mut();
            if walks.len() > 1 {
                walks.pop_front()
            } else {
                walks.front().cloned()
            }
        };
        self.record(Event::Walk);
        walk.unwrap_or_default()
    }

    fn pin(&self, pid: u32) -> Option<u32> {
        self.live.borrow().contains_key(&pid).then_some(pid)
    }

    fn start(&self, pid: u32) -> Option<u64> {
        self.live.borrow().get(&pid).copied()
    }

    fn send(&self, handle: &u32, signal: libc::c_int) {
        let pid = libc::pid_t::try_from(*handle).expect("pid");
        self.record(Event::Signal(pid, signal));
    }

    fn signal_unreaped(&self, target: libc::pid_t, signal: libc::c_int) {
        self.record(Event::Signal(target, signal));
    }
}

fn process(pid: u32) -> Process {
    Process {
        pid,
        start: u64::from(pid),
    }
}

#[test]
fn a_pid_recycled_before_the_freeze_is_never_signaled() {
    // The worker exits and its pid goes to an unrelated process right after
    // the walk saw it; later walks no longer list it.
    let table = Scripted::new(
        vec![vec![process(101), process(102)], vec![process(102)]],
        Some((Event::Walk, 101)),
    );
    tear_down(&table, ROOT);
    assert_eq!(table.signals_to(101), Vec::<libc::c_int>::new());
    assert_eq!(table.signals_to(102), vec![libc::SIGSTOP, libc::SIGKILL]);
}

#[test]
fn a_pid_recycled_after_the_group_kill_is_not_killed_again() {
    // The group kill ends worker 101, its parent reaps it, and the pid is
    // reused before the per-descendant kill.
    let table = Scripted::new(
        vec![vec![process(101)]],
        Some((Event::Signal(-ROOT_PID, libc::SIGKILL), 101)),
    );
    tear_down(&table, ROOT);
    assert_eq!(table.signals_to(101), vec![libc::SIGSTOP]);
}

#[test]
fn the_root_is_frozen_before_the_walk_and_the_walk_repeats_until_nothing_is_new() {
    let table = Scripted::new(
        vec![
            vec![process(101)],
            vec![process(101), process(102)],
            vec![process(101), process(102)],
        ],
        None,
    );
    tear_down(&table, ROOT);
    let log = table.log.borrow();
    assert_eq!(log[0], Event::Signal(ROOT_PID, libc::SIGSTOP), "{log:?}");
    assert_eq!(log.iter().filter(|event| **event == Event::Walk).count(), 3);
    assert!(log.contains(&Event::Signal(-ROOT_PID, libc::SIGKILL)));
    assert!(log.contains(&Event::Signal(ROOT_PID, libc::SIGKILL)));
    for pid in [101, 102] {
        assert_eq!(table.signals_to(pid), vec![libc::SIGSTOP, libc::SIGKILL]);
    }
}

#[test]
fn reserved_pids_and_a_root_of_init_are_never_signaled() {
    let table = Scripted::new(vec![vec![process(0), process(1)]], None);
    tear_down(&table, ROOT);
    assert!(table.signals_to(0).is_empty() && table.signals_to(1).is_empty());
    let init = Scripted::new(vec![vec![process(101)]], None);
    tear_down(&init, 1);
    assert!(init.log.borrow().is_empty());
}
