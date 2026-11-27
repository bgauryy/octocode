use super::*;

/// An over-cap file whose stream cannot be opened or read (gone, or not a
/// regular file) is unread, never a miss: whether it holds an anchor is
/// unknown.
#[test]
fn a_large_file_whose_stream_fails_is_unread() {
    let filter = AnchorFilter::new(&Prefilter::Single("needle".to_owned()))
        .ok()
        .flatten()
        .unwrap_or_else(|| unreachable!("a non-empty anchor builds a filter"));
    let dir = tempfile::tempdir().unwrap_or_else(|error| unreachable!("{error}"));
    for path in [dir.path().join("gone.rs"), dir.path().to_path_buf()] {
        assert!(
            matches!(
                filter.check_large(&path, u64::MAX),
                Anchored::Unread(SourceSkip::Unreadable(_))
            ),
            "{}",
            path.display()
        );
    }
}
