//! Ingest progress lines for a host that shows them. Lines are emitted as
//! the ingest advances: each stage's start and end, and parsed-file counts
//! at each twentieth of the inventory. They go to the call's own listener
//! when its host gave one (an MCP call that sent a progress token:
//! [`octocode_io::notice::Scope::with_progress`]), else wherever
//! [`octocode_io::notice::progress_sink`] sends them (the CLI writes them to
//! stderr); with nothing listening, nothing is emitted.
use octocode_io::notice::{Progress as Sink, progress_sink as sink};

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
        send(sink.as_ref(), || {
            format!(
                "parsing {}",
                octocode_engine::text::counted(total, "file", "files")
            )
        });
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
mod tests;
