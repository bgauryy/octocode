use super::*;

/// The file at `path`, opened, with its metadata then.
fn opened(path: &std::path::Path) -> (std::fs::File, std::fs::Metadata) {
    let file = std::fs::File::open(path).expect("open");
    let meta = file.metadata().expect("metadata");
    (file, meta)
}

/// A read no write reached is stable, and the metadata it returns
/// describes the bytes it read.
#[test]
fn a_read_no_write_reached_is_stable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("still.txt");
    std::fs::write(&path, "one version\n").expect("write");
    let (file, meta) = opened(&path);
    assert!(unchanged_since(&file, &meta));
    match Settling::default().settled(&file, &meta) {
        Settled::Stable(after) => assert_eq!(after.len(), meta.len()),
        other => panic!("{other:?}"),
    }
}

/// A read a rewrite in place reached is made again; the read after it, no
/// write reaching it, is stable.
#[test]
fn a_read_a_rewrite_reached_is_made_again_until_one_is_stable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("live.txt");
    std::fs::write(&path, "one version\n").expect("write");
    let mut settling = Settling::default();
    let (file, meta) = opened(&path);
    std::fs::write(&path, "another version, longer\n").expect("rewrite in place");
    assert!(!unchanged_since(&file, &meta));
    assert!(matches!(settling.settled(&file, &meta), Settled::Again));
    let (file, meta) = opened(&path);
    assert!(matches!(settling.settled(&file, &meta), Settled::Stable(_)));
}

/// A rewrite that keeps the file's length still moves its stamp.
#[test]
fn a_rewrite_of_the_same_length_is_seen() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("live.txt");
    std::fs::write(&path, "version one\n").expect("write");
    let (file, meta) = opened(&path);
    // A later write gets a later time; the clock behind it may tick
    // coarsely, so the rewrite is repeated until its time moves.
    while std::fs::metadata(&path).and_then(|now| now.modified()).ok() == meta.modified().ok() {
        std::fs::write(&path, "version two\n").expect("rewrite in place");
    }
    assert!(!unchanged_since(&file, &meta));
}

/// A file rewritten through every read is read again only within its
/// settle window since the first read a write reached; then no read of it
/// is proved.
#[test]
fn a_file_rewritten_through_its_settle_window_is_unsettled() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("live.txt");
    std::fs::write(&path, "one version\n").expect("write");
    let mut settling = Settling::default();
    let started = std::time::Instant::now();
    let mut window = None;
    let mut writes = 0u64;
    let outcome = loop {
        let (file, meta) = opened(&path);
        writes += 1;
        let text = if writes % 2 == 1 {
            "another version, longer\n"
        } else {
            "one version\n"
        };
        std::fs::write(&path, text).expect("rewrite in place");
        window.get_or_insert_with(|| {
            let after = std::fs::metadata(&path).expect("metadata");
            crate::stamp::SourceStamp::of(&after)
                .expect("stamp")
                .settle_for()
        });
        match settling.settled(&file, &meta) {
            Settled::Again => {}
            other => break other,
        }
    };
    assert!(matches!(outcome, Settled::Unsettled), "{outcome:?}");
    let window = window.expect("a window");
    assert!(
        started.elapsed() >= window,
        "it read again through the window: {window:?}"
    );
}

/// A read that ends for a file that kept changing names why.
#[test]
fn the_error_of_a_file_that_kept_changing_is_known() {
    let error = changed_while_read();
    assert!(is_changed_while_read(&error));
    assert!(!is_changed_while_read(&std::io::Error::other(
        "another failure"
    )));
    assert!(
        error
            .to_string()
            .contains("kept changing while it was read")
    );
}

thread_local! {
    /// A write made on this thread each time a prefix is read again, before
    /// its bytes are read, as a writer racing that read makes it; it stays
    /// while it returns `true`.
    static DURING_PREFIX_REREAD: std::cell::RefCell<Option<Box<dyn FnMut() -> bool>>> =
        std::cell::RefCell::new(None);
}

/// Makes the write registered on this thread for a prefix read again.
pub(super) fn during_prefix_reread() {
    DURING_PREFIX_REREAD.with_borrow_mut(|hook| {
        if let Some(write) = hook
            && !write()
        {
            *hook = None;
        }
    });
}

/// Appends `text` to the file at `path`, as a log writer does.
fn append(path: &std::path::Path, text: &str) {
    use std::io::Write as _;
    std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .and_then(|mut file| file.write_all(text.as_bytes()))
        .expect("append");
}

/// A whole read of the file at `path`, opened, its bytes and its metadata
/// when it was opened.
fn read_whole(path: &std::path::Path) -> (std::fs::File, std::fs::Metadata, Vec<u8>) {
    let (file, meta) = opened(path);
    let mut bytes = Vec::new();
    (&file).read_to_end(&mut bytes).expect("read");
    (file, meta, bytes)
}

/// What [`Settling::settled_read`] proves of `bytes`, read of `file`.
fn proved(file: &std::fs::File, meta: &std::fs::Metadata, bytes: &[u8]) -> Proved {
    Settling::default().settled_read(file, meta, || Some(Taken::Bytes(bytes)), &|| true)
}

/// A read of a file that only grew after it was opened is the start of one
/// version of it: proved as of the bytes it took.
#[test]
fn a_read_of_a_file_that_only_grew_is_proved_by_its_prefix() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("live.log");
    std::fs::write(&path, "line one\nline two\n").expect("write");
    let (file, meta, bytes) = read_whole(&path);
    append(&path, "line three\n");
    match proved(&file, &meta, &bytes) {
        Proved::Prefix(prefix) => {
            assert_eq!(prefix.len, bytes.len() as u64);
            assert!(prefix.bytes.is_empty(), "the caller holds the bytes");
        }
        other => panic!("{other:?}"),
    }
}

/// A read that streamed its bytes is proved by their digest, and the
/// bytes read again are kept when asked for.
#[test]
fn a_hashed_read_of_a_file_that_only_grew_is_proved_and_its_prefix_kept() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("live.log");
    let first = "line one\nline two\n";
    std::fs::write(&path, first).expect("write");
    let (file, meta, bytes) = read_whole(&path);
    append(&path, "line three\n");
    let taken = Taken::Hashed {
        len: bytes.len() as u64,
        digest: crate::digest::sha256_digest(&bytes),
        keep: true,
    };
    match Settling::default().settled_read(&file, &meta, || Some(taken), &|| true) {
        Proved::Prefix(prefix) => assert_eq!(prefix.bytes, first.as_bytes()),
        other => panic!("{other:?}"),
    }
}

/// A read a save in place tore (the old version's start, the new one's
/// rest) is no version's prefix, though the file is longer after it: it is
/// made again.
#[test]
fn a_read_a_rewrite_tore_is_not_proved_by_its_prefix() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("live.txt");
    std::fs::write(&path, "one version\n").expect("write");
    let (file, meta) = opened(&path);
    let mut bytes = vec![0u8; "one version\n".len()];
    (&file)
        .read_exact(&mut bytes)
        .expect("read the first version");
    std::fs::write(&path, "another version, a longer one\n").expect("rewrite in place");
    (&file).read_to_end(&mut bytes).expect("read on");
    assert!(matches!(proved(&file, &meta, &bytes), Proved::Again));
}

/// A file rewritten shorter than the read is not a version it grew to.
#[test]
fn a_read_of_a_file_rewritten_shorter_is_not_proved() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("live.txt");
    std::fs::write(&path, "line one\nline two\n").expect("write");
    let (file, meta, bytes) = read_whole(&path);
    std::fs::write(&path, "line\n").expect("rewrite in place");
    assert!(matches!(proved(&file, &meta, &bytes), Proved::Again));
}

/// A read whose last line the file then went on with cuts that line: it is
/// no prefix that ends at a line break, so it is made again.
#[test]
fn a_prefix_without_a_whole_last_line_is_not_proved() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("live.log");
    std::fs::write(&path, "line one\nline tw").expect("write");
    let (file, meta, bytes) = read_whole(&path);
    append(&path, "o\nline three\n");
    assert!(matches!(proved(&file, &meta, &bytes), Proved::Again));
}

/// A write that moves the prefix read again proves nothing of it: it is
/// read again, and the read after it, which no write moved, proves the read.
#[test]
fn a_prefix_read_again_while_written_is_read_again_until_it_settles() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("live.log");
    std::fs::write(&path, "line one\n").expect("write");
    let (file, meta, bytes) = read_whole(&path);
    append(&path, "line two\n");
    let (target, mut rereads) = (path.clone(), 0);
    DURING_PREFIX_REREAD.with_borrow_mut(|hook| {
        *hook = Some(Box::new(move || {
            rereads += 1;
            append(&target, "line written while the prefix is read\n");
            rereads < 1
        }));
    });
    let proof = proved(&file, &meta, &bytes);
    assert!(DURING_PREFIX_REREAD.with_borrow(Option::is_none));
    assert!(matches!(proof, Proved::Prefix(_)), "{proof:?}");
}

/// A file written through every read of its prefix again settles no read:
/// once its window passed, none is proved.
#[test]
fn a_prefix_written_through_every_read_again_is_unsettled() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("live.log");
    std::fs::write(&path, "line one\n").expect("write");
    let (file, meta, bytes) = read_whole(&path);
    append(&path, "line two\n");
    let target = path.clone();
    DURING_PREFIX_REREAD.with_borrow_mut(|hook| {
        *hook = Some(Box::new(move || {
            append(&target, "line written while the prefix is read\n");
            true
        }));
    });
    let started = std::time::Instant::now();
    let proof = proved(&file, &meta, &bytes);
    DURING_PREFIX_REREAD.with_borrow_mut(|hook| *hook = None);
    assert!(matches!(proof, Proved::Unsettled), "{proof:?}");
    let window = crate::stamp::SourceStamp::of(&std::fs::metadata(&path).expect("metadata"))
        .expect("stamp")
        .settle_for();
    assert!(started.elapsed() >= window, "read again through {window:?}");
}

/// A write that moved only the file's times leaves its bytes: read again,
/// they are the file whole, and the read is proved whole.
#[test]
fn bytes_read_again_that_are_the_file_whole_prove_the_read_whole() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("still.txt");
    std::fs::write(&path, "one version, no line break").expect("write");
    let (file, meta, bytes) = read_whole(&path);
    let later = meta.modified().expect("mtime") + std::time::Duration::from_secs(5);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .and_then(|handle| handle.set_modified(later))
        .expect("touch");
    assert!(!unchanged_since(&file, &meta), "the stamp moved");
    match proved(&file, &meta, &bytes) {
        Proved::Whole(after) => assert_eq!(after.len(), bytes.len() as u64),
        other => panic!("{other:?}"),
    }
}

/// A read that says nothing of what it took is made again as before.
#[test]
fn a_moved_read_that_names_nothing_it_took_is_made_again() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("live.log");
    std::fs::write(&path, "line one\n").expect("write");
    let (file, meta, _) = read_whole(&path);
    append(&path, "line two\n");
    let proof = Settling::default().settled_read(&file, &meta, || None, &|| true);
    assert!(matches!(proof, Proved::Again), "{proof:?}");
}
