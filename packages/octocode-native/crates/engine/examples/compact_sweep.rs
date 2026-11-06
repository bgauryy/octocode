//! Corpus sweep for the compact view and the parse work bound. For every file
//! under the given roots: the compact view must keep the source's tokens
//! (`minify::verify`), and a clean parse's work per byte is recorded per
//! grammar, the measurement behind `REPORTS_PER_KIB` and `READS_PER_CHUNK`
//! in `signatures/extractor/parse.rs`. Run on an optimized build:
//! `cargo run --profile profiling -p octocode-engine --example compact_sweep -- <root>...`
//! (`skills-dev/octocode-dev/scripts/compact_sweep.py` wraps it).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use octocode_engine::minify::verify::{parse_work, verify};

const SKIPPED_DIRS: &[&str] = &["target", "node_modules", "dist", "out", "build", "coverage"];

fn walk(root: &Path, found: &mut Vec<PathBuf>, unread: &mut Vec<String>) {
    if root.is_file() {
        found.push(root.into());
        return;
    }
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            unread.push(format!("{}: {error}", root.display()));
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                unread.push(format!("{}: {error}", root.display()));
                continue;
            }
        };
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if !path.is_dir() {
            found.push(path);
        } else if !name.starts_with('.') && !SKIPPED_DIRS.contains(&name.as_str()) {
            walk(&path, found, unread);
        }
    }
}

#[derive(Default)]
struct Totals {
    failed: Vec<String>,
    unread: Vec<String>,
    sizes: BTreeMap<String, (usize, usize, usize)>,
    whitespace_only: BTreeMap<&'static str, usize>,
    work: BTreeMap<&'static str, (f64, String, f64, String, usize)>,
}

fn sweep(path: &Path, totals: &Mutex<Totals>) {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            totals
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .unread
                .push(format!("{}: {error}", path.display()));
            return;
        }
    };
    let name = path.to_string_lossy().into_owned();
    let extension = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let checked = std::panic::catch_unwind(|| (verify(&name, &source), parse_work(&name, &source)));
    let mut totals = totals
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Ok((checked, work)) = checked else {
        totals.failed.push(format!("{name}: panicked"));
        return;
    };
    match checked {
        Ok(verified) => {
            if let Some(reason) = verified.whitespace_only {
                *totals.whitespace_only.entry(reason.reason()).or_default() += 1;
                if reason == octocode_engine::minify::apply::WhitespaceOnly::NoGrammar {
                    return;
                }
            }
            let size = totals.sizes.entry(extension).or_default();
            size.0 += source.len();
            size.1 += verified.view_bytes;
            size.2 += 1;
        }
        Err(change) => totals.failed.push(format!("{name}: {change}")),
    }
    if let Some(work) = work {
        let row = totals.work.entry(work.grammar).or_default();
        row.4 += 1;
        if work.reports_per_byte > row.0 {
            (row.0, row.1) = (work.reports_per_byte, name.clone());
        }
        if work.reads_per_chunk > row.2 {
            (row.2, row.3) = (work.reads_per_chunk, name);
        }
    }
}

fn main() -> std::process::ExitCode {
    let mut files = Vec::new();
    let mut unread = Vec::new();
    let roots = std::env::args().skip(1).collect::<Vec<_>>();
    if roots.is_empty() {
        eprintln!("usage: compact_sweep <file-or-directory>...");
        return std::process::ExitCode::from(2);
    }
    for root in roots {
        walk(Path::new(&root), &mut files, &mut unread);
    }
    let totals = Mutex::new(Totals {
        unread,
        ..Totals::default()
    });
    let workers = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                while let Some(path) =
                    files.get(next.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
                {
                    sweep(path, &totals);
                }
            });
        }
    });
    let totals = totals
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    println!("files {}", files.len());
    for (grammar, (reports, worst_reports, reads, worst_reads, count)) in &totals.work {
        println!(
            "work {grammar}: {count} clean files, max {reports:.4} reports/byte ({worst_reports}), max {reads:.2} reads/chunk ({worst_reads})"
        );
    }
    for (extension, (source, view, count)) in &totals.sizes {
        let saved = 100.0 * source.saturating_sub(*view) as f64 / (*source).max(1) as f64;
        println!("saved {extension}: {count} files, {source} -> {view} bytes, {saved:.1}%");
    }
    for (reason, count) in &totals.whitespace_only {
        println!("whitespace-only {reason}: {count}");
    }
    for failure in &totals.failed {
        println!("CHANGED {failure}");
    }
    for failure in &totals.unread {
        println!("UNREAD {failure}");
    }
    println!("unread {}", totals.unread.len());
    println!("changed {}", totals.failed.len());
    if totals.failed.is_empty() && totals.unread.is_empty() {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
