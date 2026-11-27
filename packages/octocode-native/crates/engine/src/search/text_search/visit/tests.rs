//! How a walk keeps the hits of a file written while it is searched: a file
//! that only grew keeps the hits of the bytes the search read when they are
//! the start of one version (and a judged search, by its judge's verdict on
//! those bytes); a save in place is searched again; a file that keeps
//! changing is counted as such, never as a failure of the pattern.

use crate::search::text_search::{TextSearchPathFilter, search_cancellable};
use crate::types::{JudgeHeld, SourceJudge, SourceValues, TextSearchOptions, TextSearchResult};
use std::path::Path;
use std::sync::Arc;

struct AllowAll;

impl TextSearchPathFilter for AllowAll {
    fn allows(&self, _: &Path, _: bool) -> bool {
        true
    }
}

/// A search of `root` for `pattern` whose kept files carry the digest of
/// the bytes they were searched in, as a local search's do.
fn hashed(root: &Path, pattern: &str) -> TextSearchOptions {
    TextSearchOptions {
        path: root.to_string_lossy().into_owned(),
        pattern: pattern.to_owned(),
        values: SourceValues::Hashed,
        ..Default::default()
    }
}

fn search(options: TextSearchOptions) -> TextSearchResult {
    search_cancellable(options, Arc::new(AllowAll), &|| false).expect("search")
}

/// Appends `text` to the file at `path`, as a log writer does.
fn append(path: &Path, text: &str) {
    use std::io::Write as _;
    std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .and_then(|mut file| file.write_all(text.as_bytes()))
        .expect("append");
}

/// Runs `body` with `write` registered for the file at `path`, made after
/// each read of it ends and before the read is proved, while it returns
/// `true`.
#[cfg(unix)]
fn after_each_read<T>(
    path: &Path,
    write: impl FnMut() -> bool + Send + 'static,
    body: impl FnOnce() -> T,
) -> T {
    use std::os::unix::fs::MetadataExt as _;
    let inode = std::fs::metadata(path).expect("metadata").ino();
    let races = || {
        crate::text::BEFORE_PROOF
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    };
    races().push((inode, Box::new(write)));
    let out = body();
    races().retain(|(registered, _)| *registered != inode);
    out
}

/// A judge that serves `hidden` as stars wherever a file holds it, whole
/// or held, as a secret judge replaces a secret. No literal it is asked
/// about takes the stars, so a file's own bytes are searched first.
fn starring(hidden: &'static str, held: bool) -> SourceJudge {
    let star = move |text: &str| {
        text.contains(hidden).then(|| crate::types::ServedText {
            text: text.replace(hidden, &"*".repeat(hidden.len())).into_bytes(),
            written: text
                .match_indices(hidden)
                .map(|(at, _)| at..at + hidden.len())
                .collect(),
        })
    };
    SourceJudge {
        serve: Arc::new(
            move |_: &Path, file: &std::fs::File, _: &(dyn Fn() -> bool + Sync)| {
                use std::io::Read as _;
                let mut text = String::new();
                (&*file).read_to_string(&mut text)?;
                Ok(star(&text))
            },
        ),
        writes: Arc::new(|literal: &str, _: bool| literal.contains('*')),
        held: held.then(|| JudgeHeld {
            limit: 1 << 20,
            serve: Arc::new(move |_: &Path, bytes: &[u8]| star(&String::from_utf8_lossy(bytes))),
        }),
    }
}

/// A log appended to after every read keeps the hits of the bytes one read
/// took, every line it held and none written after, and the search names
/// it as grown, with those bytes; nothing of it is an error.
#[cfg(unix)]
#[test]
fn a_file_that_only_grew_keeps_the_hits_of_its_read() {
    let root = tempfile::tempdir().expect("root");
    let path = root.path().join("live.log");
    let log = "needle one\nneedle two\n";
    std::fs::write(&path, log).expect("fixture");
    let target = path.clone();
    let result = after_each_read(
        &path,
        move || {
            append(&target, "needle written after\n");
            true
        },
        || search(hashed(root.path(), "needle")),
    );
    assert_eq!(result.stats.error_count, Some(0), "{:?}", result.stats);
    let [file] = result.files.as_slice() else {
        panic!("one kept file: {:?}", result.files);
    };
    let values = file
        .matches
        .iter()
        .map(|hit| hit.value.as_str())
        .collect::<Vec<_>>();
    assert_eq!(values, ["needle one", "needle two"]);
    let grown = result.stats.grown_files.expect("named as grown");
    assert_eq!(grown.len(), 1);
    assert!(grown[0].path.ends_with("live.log"), "{grown:?}");
    assert_eq!(grown[0].bytes, log.len() as u64);
}

/// A search that keeps no digest of what it read cannot prove its read the
/// start of a version: a log appended to after every read never settles, and
/// it is counted as a file that kept changing.
#[cfg(unix)]
#[test]
fn a_grown_file_a_search_kept_no_digest_of_is_counted_as_changing() {
    let root = tempfile::tempdir().expect("root");
    let path = root.path().join("live.log");
    std::fs::write(&path, "needle one\n").expect("fixture");
    let target = path.clone();
    let mut options = hashed(root.path(), "needle");
    options.values = SourceValues::Owned;
    let result = after_each_read(
        &path,
        move || {
            append(&target, "needle written after\n");
            true
        },
        || search(options),
    );
    assert!(result.files.is_empty(), "{:?}", result.files);
    assert_eq!(
        result.stats.changed_error_count,
        Some(1),
        "{:?}",
        result.stats
    );
    assert_eq!(result.stats.pattern_error_count, None);
}

/// A save in place after the search read the file, longer and starting with
/// other bytes, is no growth: the file is searched again, and its hits are
/// the settled version's.
#[cfg(unix)]
#[test]
fn a_save_in_place_after_the_search_is_searched_again() {
    let root = tempfile::tempdir().expect("root");
    let path = root.path().join("live.txt");
    std::fs::write(&path, "needle first\n").expect("fixture");
    let target = path.clone();
    let result = after_each_read(
        &path,
        move || {
            std::fs::write(&target, "needle second\nneedle third\n").expect("rewrite in place");
            false
        },
        || search(hashed(root.path(), "needle")),
    );
    let values = result.files[0]
        .matches
        .iter()
        .map(|hit| hit.value.as_str())
        .collect::<Vec<_>>();
    assert_eq!(values, ["needle second", "needle third"]);
    assert_eq!(result.stats.grown_files, None);
}

/// A judged search keeps a grown file's hits by its judge's verdict on the
/// bytes the search read, held: the hits are of the text it serves of them.
/// A judge with no verdict on held bytes keeps none: the file is counted as
/// one that kept changing.
#[cfg(unix)]
#[test]
fn a_judged_search_keeps_a_grown_file_by_the_verdict_on_its_read() {
    for held in [true, false] {
        let root = tempfile::tempdir().expect("root");
        let path = root.path().join("live.log");
        std::fs::write(&path, "needle FAKESECRET one\nneedle two\n").expect("fixture");
        let target = path.clone();
        let mut options = hashed(root.path(), "needle");
        options.judge = Some(starring("FAKESECRET", held));
        let result = after_each_read(
            &path,
            move || {
                append(&target, "needle written after\n");
                true
            },
            || search(options),
        );
        if held {
            let values = result.files[0]
                .matches
                .iter()
                .map(|hit| hit.value.as_str())
                .collect::<Vec<_>>();
            assert_eq!(values, ["needle ********** one", "needle two"]);
            assert!(result.stats.grown_files.is_some(), "{:?}", result.stats);
        } else {
            assert!(result.files.is_empty(), "{:?}", result.files);
            assert_eq!(result.stats.changed_error_count, Some(1));
        }
    }
}

/// A search that fails while its file is rewritten under it (a judge that
/// found the file changed under its read) is made again; a file that keeps
/// failing so is counted as one that kept changing while it was read, not
/// as a file the pattern failed in.
#[test]
fn a_search_that_fails_while_its_file_keeps_changing_is_no_pattern_failure() {
    let root = tempfile::tempdir().expect("root");
    let path = root.path().join("live.log");
    std::fs::write(&path, "needle one\n").expect("fixture");
    let mut options = hashed(root.path(), "needle");
    let mut judge = starring("FAKESECRET", false);
    let writes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    judge.serve = Arc::new(
        move |path: &Path, _: &std::fs::File, _: &(dyn Fn() -> bool + Sync)| {
            let n = writes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            std::fs::write(path, ["needle one\n", "needle one, longer\n"][n % 2])?;
            Err(std::io::Error::other(
                "the source changed while it was read",
            ))
        },
    );
    options.judge = Some(judge);
    let result = search(options);
    assert!(result.files.is_empty(), "{:?}", result.files);
    assert_eq!(result.stats.pattern_error_count, None, "{:?}", result.stats);
    assert_eq!(
        result.stats.changed_error_count,
        Some(1),
        "{:?}",
        result.stats
    );
    assert!(
        result
            .stats
            .first_error
            .as_deref()
            .is_some_and(|error| error.contains("kept changing while it was read")),
        "{:?}",
        result.stats
    );
}
