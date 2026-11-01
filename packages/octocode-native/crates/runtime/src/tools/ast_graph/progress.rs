//! Ingest progress lines for a host that shows them. Lines are emitted as
//! the ingest advances: each stage's start and end, and parsed-file counts
//! at each twentieth of the inventory. They go to the call's own listener
//! when its host gave one (an MCP call that sent a progress token:
//! [`octocode_io::notice::Scope::with_progress`]), else to the process sink
//! (the CLI writes them to stderr); with neither, nothing is emitted.
use std::sync::{Arc, RwLock};

type Sink = Arc<dyn Fn(&str) + Send + Sync>;

static SINK: RwLock<Option<Sink>> = RwLock::new(None);

/// Sends every later ingest's progress lines to `sink`.
pub fn report_to(sink: impl Fn(&str) + Send + Sync + 'static) {
    *SINK
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Arc::new(sink));
}

/// Where the running call's lines go: its scope's listener, else the
/// process sink.
fn sink() -> Option<Sink> {
    octocode_io::notice::Scope::current()
        .and_then(|scope| scope.progress())
        .or_else(|| {
            SINK.read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        })
}

fn send(sink: Option<&Sink>, line: impl FnOnce() -> String) {
    if let Some(sink) = sink {
        sink(&format!("graphBuild: {}", line()));
    }
}

/// Emits one progress line.
pub(super) fn emit(line: impl FnOnce() -> String) {
    send(sink().as_ref(), line);
}

/// Counts parsed files and emits the count each time it crosses another
/// twentieth of `total`.
pub(super) struct Parsing {
    /// The call's sink, taken where the count starts: files finish on the
    /// parse pool's threads, outside the call's scope.
    sink: Option<Sink>,
    total: usize,
    step: usize,
    done: std::sync::atomic::AtomicUsize,
}

impl Parsing {
    pub(super) fn new(total: usize) -> Self {
        let sink = sink();
        send(sink.as_ref(), || format!("parsing {total} files"));
        Self {
            sink,
            total,
            step: total.div_ceil(20).max(1),
            done: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    pub(super) fn file_done(&self) {
        let done = self.done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        if done.is_multiple_of(self.step) && done < self.total {
            send(self.sink.as_ref(), || {
                format!("parsed {done}/{} files", self.total)
            });
        }
    }

    /// The last parse count, which also starts linking: the next line
    /// comes once the graph is linked and projected.
    pub(super) fn finish(&self) {
        let done = self.done.load(std::sync::atomic::Ordering::Relaxed);
        send(self.sink.as_ref(), || {
            format!("parsed {done}/{} files; linking", self.total)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
