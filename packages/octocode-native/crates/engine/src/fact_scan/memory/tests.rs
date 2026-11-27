use super::*;

fn base(_: &GraphFactsDocument) -> usize {
    0
}

#[test]
fn draws_share_one_budget_and_the_peak_never_passes_it() {
    let memory = ScanMemory::new(100, &base);
    let (a, b) = (FileHold::new(&memory), FileHold::new(&memory));
    a.draw(60).expect("within the budget");
    let refused = b.draw(50);
    assert!(refused.is_err(), "two files together pass the budget");
    assert!(
        b.contended(),
        "refused only because another file holds memory"
    );
    assert_eq!(b.needed(), 50);
    b.draw(40).expect("what is left");
    assert_eq!(memory.peak(), 100);
    drop(a);
    b.draw(50).expect("released by the other file");
    assert!(memory.peak() <= 100);
}

#[test]
fn a_file_alone_past_the_budget_is_refused_without_contention() {
    let memory = ScanMemory::new(100, &base);
    memory.commit(30);
    let hold = FileHold::new(&memory);
    assert!(hold.draw(80).is_err());
    assert!(!hold.contended(), "nothing else holds memory");
    assert_eq!(hold.needed(), 110, "what was kept plus what it would hold");
}

#[test]
fn released_and_temporary_bytes_return_to_the_scan() {
    let memory = ScanMemory::new(100, &base);
    let hold = FileHold::new(&memory);
    hold.draw(70).unwrap();
    hold.release(70);
    hold.draw(100).expect("the release returned it");
    drop(hold);
    let other = FileHold::new(&memory);
    other.draw(100).expect("a dropped hold returns all it held");
}

/// A file refused only because another holds the memory runs again alone:
/// it waits for the other to finish, and then has the whole budget.
#[test]
fn a_contended_file_runs_again_alone_once_the_others_release() {
    let memory = ScanMemory::new(100, &base);
    std::thread::scope(|scope| {
        let first = FileHold::new(&memory);
        first.draw(90).unwrap();
        let (go, ready) = std::sync::mpsc::channel();
        let shared = &memory;
        let waiter = scope.spawn(move || {
            let memory = shared;
            let hold = FileHold::new(memory);
            assert!(hold.draw(50).is_err() && hold.take_contended());
            go.send(()).unwrap();
            memory.alone(&hold, || hold.draw(100).is_ok())
        });
        ready.recv().unwrap();
        // The waiter cannot run while this file holds memory.
        while memory.lock().waiting == 0 {
            std::thread::yield_now();
        }
        assert!(memory.lock().solo.eq(&false));
        drop(first);
        assert!(waiter.join().unwrap(), "alone, it has the whole budget");
    });
    assert_eq!(memory.peak(), 100);
}

/// However many files run at once, what they hold together stays within
/// the budget.
#[test]
fn parallel_files_never_hold_more_than_the_budget() {
    let memory = ScanMemory::new(1_000, &base);
    std::thread::scope(|scope| {
        for worker in 0..8 {
            let memory = &memory;
            scope.spawn(move || {
                for round in 0..200 {
                    let hold = FileHold::new(memory);
                    let size = 50 + (worker * 37 + round * 11) % 400;
                    let mut drew = hold.draw(size).is_ok();
                    if !drew && hold.take_contended() {
                        drew = memory.alone(&hold, || hold.draw(size).is_ok());
                    }
                    assert!(drew, "every size fits alone");
                }
            });
        }
    });
    assert!(memory.peak() <= 1_000, "{}", memory.peak());
}

#[test]
fn the_first_refusal_is_kept() {
    let memory = ScanMemory::new(10, &base);
    memory.refuse("a.ts", 20);
    memory.refuse("b.ts", 30);
    assert_eq!(
        memory.refused(),
        Some(Refusal {
            relative_path: "a.ts".into(),
            needed: 20
        })
    );
}

/// Small draws are served from the file's reserved credit: past its first
/// chunk, a file asks the scan for nothing until that chunk is used.
#[test]
fn small_draws_come_from_one_reserved_chunk() {
    let budget = 64 << 20;
    let memory = ScanMemory::new(budget, &base);
    let hold = FileHold::new(&memory);
    hold.draw(10).unwrap();
    assert_eq!(memory.peak(), memory.chunk(), "one chunk reserved");
    for _ in 0..1_000 {
        hold.draw(1_000).unwrap();
    }
    assert_eq!(memory.peak(), memory.chunk(), "served from the credit");
    hold.release(1_000_000);
    hold.draw(1_000_000).unwrap();
    assert_eq!(
        memory.peak(),
        memory.chunk(),
        "released bytes are credit again"
    );
    drop(hold);
    let other = FileHold::new(&memory);
    other
        .draw(budget)
        .expect("a dropped hold returns its credit");
}
