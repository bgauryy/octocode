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
