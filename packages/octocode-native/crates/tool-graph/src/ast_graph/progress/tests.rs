use super::*;
use std::sync::Arc;

/// A call whose host listens gets its own progress lines, those of
/// files finished on other threads included, and no other call's.
#[test]
fn a_calls_progress_goes_to_its_own_listener() {
    let lines = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let seen = Arc::clone(&lines);
    let others = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let heard = Arc::clone(&others);
    let _other = octocode_io::notice::Scope::with_progress(move |line| {
        heard.lock().expect("lines").push(line.to_owned());
    });
    let call = octocode_io::notice::Scope::with_progress(move |line| {
        seen.lock().expect("lines").push(line.to_owned());
    });
    let parsing = {
        let _in = call.enter();
        emit(|| "scanning".to_owned());
        Parsing::new(40)
    };
    std::thread::scope(|threads| {
        for _ in 0..4 {
            threads.spawn(|| (0..10).for_each(|_| parsing.file_done()));
        }
    });
    parsing.finish();
    let lines = lines.lock().expect("lines").clone();
    assert_eq!(
        lines.first().map(String::as_str),
        Some("graphBuild: scanning")
    );
    assert_eq!(lines[1], "graphBuild: parsing 40 files");
    assert_eq!(
        lines.last().map(String::as_str),
        Some("graphBuild: parsed 40/40 files; linking")
    );
    assert_eq!(
        lines.len(),
        2 + 19 + 1,
        "one count per twentieth: {lines:?}"
    );
    assert!(
        others.lock().expect("lines").is_empty(),
        "not another call's"
    );
}
