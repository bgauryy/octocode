//! Lock-poison policy: a poisoned lock is recovered with `into_inner`, never
//! skipped. Skipping silently drops work (a diagnostic, a cache entry), and on
//! coordination state it leaves waiters hung until their timeout, because the
//! failure path that would notify them is the code being skipped.

use std::path::{Path, PathBuf};

include!("../src/tools/test_support/production_source.rs");

/// Production files allowed a non-recovering lock, each with its reason.
const ALLOWED: &[&str] = &[
    // `lsp_search/scope.rs` and its split children, allowed as one file before
    // the split. The walk marks itself abandoned and quits: an explicit
    // failure, not a skip.
    "runtime/src/tools/lsp_search/scope.rs",
    "runtime/src/tools/lsp_search/scope/scan.rs",
    "runtime/src/tools/lsp_search/scope/text.rs",
];

/// A guard taken from `lock()`, `read()` or `write()` whose `Err` (a
/// poisoned lock) is dropped: `.ok()`, `.map(`, `let Ok(..) = .. else` or
/// `if let`/`while let Ok(..) = ..`.
fn skip_pattern() -> regex::Regex {
    regex::Regex::new(concat!(
        r"\.(lock|read|write)\(\)\s*\.(ok|map)\(",
        r"|let\s+Ok\([^)]*\)\s*=\s*[^;{]*\.(lock|read|write)\(\)\s*else",
        r"|(if|while)\s+let\s+Ok\([^)]*\)\s*=\s*[^;{]*\.(lock|read|write)\(\)\s*\{",
    ))
    .expect("static pattern")
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn the_skip_pattern_recognizes_every_skip_shape() {
    // Gate integrity: a pattern that matched nothing would pass vacuously.
    let pattern = skip_pattern();
    for bad in [
        "let x = m.lock().ok()?;",
        "m.lock()\n            .ok()\n            .and_then(|g| g.get(k))",
        "let Ok(mut g) = self.state.lock() else {",
        "m.lock().map(|mut g| std::mem::take(&mut *g)).unwrap_or_default()",
        "if let Ok(mut g) = self.state.lock() {",
        "while let Ok(g) = m.lock() {",
        "let cached = cache.read().ok()?;",
        "if let Ok(mut map) = CACHE.write() {",
        "let Ok(map) = CACHE.read() else {",
    ] {
        assert!(pattern.is_match(bad), "{bad}");
    }
    for good in [
        "m.lock().unwrap_or_else(PoisonError::into_inner)",
        "if let Ok(bytes) = std::fs::read(path) {",
        "if let Ok(n) = file.read(&mut buf) {",
    ] {
        assert!(!pattern.is_match(good), "{good}");
    }
}

#[test]
fn no_production_lock_is_skipped_when_poisoned() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let pattern = skip_pattern();
    let mut offenders = Vec::new();
    for krate in ["engine", "github", "io", "runtime", "cli", "runtime-napi"] {
        let mut files = Vec::new();
        rust_sources(&crates.join(krate).join("src"), &mut files);
        for file in files {
            let relative = file
                .strip_prefix(&crates)
                .unwrap_or(&file)
                .to_string_lossy()
                .replace('\\', "/");
            let name = file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if name.contains("test") || ALLOWED.contains(&relative.as_str()) {
                continue;
            }
            // Test-only items may use any shape; prose and literals hold none.
            let production = code_only(&production_source(&file));
            for found in pattern.find_iter(&production) {
                let line = production[..found.start()].lines().count();
                offenders.push(format!("{relative}:{line}"));
            }
        }
    }
    assert!(offenders.is_empty(), "skip-on-poison locks: {offenders:#?}");
}
