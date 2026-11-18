//! Digests of the bytes a kept file was searched in.

use super::*;

/// SHA-256 of a fixture file's bytes, for comparing with a search digest.
fn file_digest(path: &Path) -> [u8; 32] {
    Sha256::digest(fs::read(path).expect("fixture bytes")).into()
}

/// A digest-asking search hashes each kept file after its search, and the
/// digest equals a separate hash of the file; without the option nothing is
/// hashed.
#[test]
fn kept_files_carry_the_digest_of_the_bytes_searched() {
    let t = TmpDir::new();
    let long = format!(
        "needle head\n{}needle tail\n",
        "filler line\n".repeat(20_000)
    );
    t.write("long.txt", long);
    t.write("short.txt", "one needle\n");
    t.write("none.txt", "nothing here\n");
    for view in ["normal", "count", "files", "without"] {
        let mut options = opts(t.path(), "needle");
        options.values = SourceValues::Hashed;
        options.output = match view {
            "count" => SearchOutput::CountMatches,
            "files" => SearchOutput::FilesWithMatch,
            "without" => SearchOutput::FilesWithoutMatch,
            _ => SearchOutput::Lines,
        };
        let result = search(options).expect("search");
        assert!(!result.files.is_empty(), "{view}");
        for file in &result.files {
            let path = Path::new(&file.path);
            let source = file.source.expect("digest of a kept file");
            assert_eq!(source.digest, file_digest(path), "{view} {}", file.path);
            let meta = fs::metadata(path).expect("meta");
            assert_eq!(source.size, meta.len());
            assert_eq!(source.modified, Some(meta.modified().expect("mtime")));
        }
    }
    let unhashed = search(opts(t.path(), "needle")).expect("search");
    assert!(unhashed.files.iter().all(|file| file.source.is_none()));
}

/// A kept file's digest proves the opened file whole: a file whose length
/// moved after it was opened carries no digest.
#[test]
fn a_file_that_moved_after_opening_has_no_digest() {
    let t = TmpDir::new();
    let path = t.write("grow.txt", "needle\n");
    let file = std::fs::File::open(&path).expect("open");
    let meta = file.metadata().expect("meta");
    let source = hashed_source(&file, &meta).expect("unchanged file");
    assert_eq!(source.digest, file_digest(&path));
    assert_eq!(source.size, meta.len());
    fs::write(&path, "needle\nmore\n").expect("grow");
    assert_eq!(hashed_source(&file, &meta), None);
}

/// A file quit at its first NUL is searched again up to it; its digest
/// still covers every byte, including the unsearched tail after the NUL.
#[test]
fn a_binary_cut_file_digest_covers_the_whole_file() {
    let t = TmpDir::new();
    let mut bytes = b"needle before the cut\n".repeat(50);
    bytes.push(0);
    bytes.extend(b"after the cut needle\n".repeat(10_000));
    let path = t.0.join("mixed.log");
    fs::write(&path, &bytes).expect("fixture");
    let mut options = opts(t.path(), "needle");
    options.values = SourceValues::Hashed;
    let result = search(options).expect("search");
    assert_eq!(result.files.len(), 1);
    assert_eq!(result.files[0].matches.len(), 50);
    let source = result.files[0].source.expect("digest");
    assert_eq!(source.digest, file_digest(&path));
    assert_eq!(source.size, bytes.len() as u64);
}

/// A judge that rewrites its file in place before it reads it, once: the
/// same length, its modification time put back, as a write between two
/// reads of one open file that its timestamps do not show. It replaces
/// nothing, so the file is searched as its own bytes.
fn rewriting_judge(rewrite: &'static str) -> crate::types::SourceJudge {
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    crate::types::SourceJudge {
        serve: Arc::new(
            move |path: &Path, file: &std::fs::File, _: &(dyn Fn() -> bool + Sync)| {
                if !done.swap(true, Ordering::Relaxed) {
                    let modified = file.metadata()?.modified()?;
                    fs::write(path, rewrite)?;
                    fs::File::options()
                        .write(true)
                        .open(path)?
                        .set_modified(modified)?;
                }
                Ok(None)
            },
        ),
        writes: Arc::new(|_: &str, _: bool| false),
        held: None,
    }
}

/// The digest a kept file carries is of the bytes its values were searched
/// in. A rewrite of the open file while it is searched, which its length and
/// modification time do not show, still moves its change time (Unix): the
/// file is searched again, and its values and digest are both of the bytes
/// that search read, never of two versions.
#[test]
fn the_digest_is_of_the_bytes_the_values_were_searched_in() {
    let t = TmpDir::new();
    let scanned = "head line\nneedle one\ntail line\n";
    let rewritten = "HEAD LINE\nneedle ONE\nTAIL LINE\n";
    assert_eq!(scanned.len(), rewritten.len());
    let path = t.write("live.txt", scanned);
    let mut options = opts(t.path(), "needle");
    options.values = SourceValues::Hashed;
    options.context = crate::types::SearchContext::symmetric(1);
    options.judge = Some(rewriting_judge(rewritten));
    let result = search(options).expect("search");
    assert_eq!(fs::read_to_string(&path).expect("rewritten"), rewritten);
    let file = &result.files[0];
    // The values show one version; the digest must be of that one.
    let version = if file.matches[0].value.contains("HEAD LINE") {
        rewritten
    } else {
        scanned
    };
    #[cfg(unix)]
    assert_eq!(
        version, rewritten,
        "the rewrite was seen: {:?}",
        file.matches
    );
    let source = file.source.expect("digest");
    assert_eq!(
        source.digest,
        <[u8; 32]>::from(Sha256::digest(version.as_bytes()))
    );
    assert_eq!(source.size, version.len() as u64);
}

/// A walk stopped while a file was searched keeps that file with the digest
/// of the bytes it was searched in, as an unstopped walk does: its values
/// are never left without the bytes that prove them.
#[test]
fn a_file_kept_by_a_stopped_walk_carries_its_digest() {
    static STOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let t = TmpDir::new();
    let body = "needle one\nanother line\n";
    let path = t.write("kept.txt", body);
    let mut options = opts(t.path(), "needle");
    options.values = SourceValues::Deferred;
    // The walk is stopped as the file's hit is recorded, before its search ends.
    options.inspect_deferred = Some(crate::types::DeferredInspector(Arc::new(
        |_: &Path, hit: &mut crate::types::TextSearchMatch| {
            STOP.store(true, Ordering::Relaxed);
            hit.value = String::new();
            hit.deferred = Some(0);
        },
    )));
    let state = CollectState::new();
    let control = super::super::collect::WalkControl {
        cancelled: &|| false,
    };
    let (_, scope) = super::super::collect::walk_scope(&options, Mode::Normal, &state, control)
        .expect("walk scope");
    let matcher = rust_matcher_builder(&options)
        .build(&rust_spelling(&options, &options.pattern))
        .expect("matcher");
    let held = Arc::new(Held::new(None));
    let mut searcher =
        super::super::collect::file_searcher(&scope, Mode::Normal, (&matcher, &[]), (&STOP, &held));
    let mut recs = super::super::collect::WorkerRecs {
        local: Vec::new(),
        state: &state,
    };
    scope.search_entry(&state, &path, &mut searcher, &mut recs);
    assert!(STOP.load(Ordering::Relaxed), "the walk was stopped");
    let kept = recs.local.first().expect("the file is kept");
    let source = kept.source.expect("digest of a kept file");
    assert_eq!(
        source.digest,
        <[u8; 32]>::from(Sha256::digest(body.as_bytes()))
    );
}
