use super::*;

fn write(path: &Path, text: &str) {
    std::fs::write(path, text).expect("write");
}

#[test]
fn words_are_bounded_by_non_identifier_bytes() {
    // A punctuation end needs no boundary: `appState.name` spells `.name`.
    assert!(spells_word(b"appState.f()", b".f", true, true));
    assert!(spells_word(b"f();", b"f", true, true));
    assert!(spells_word(b"a.f\n", b"f", true, true));
    assert!(spells_word(b"a f", b"f", true, true));
    assert!(
        !spells_word(b"a f", b"f", true, false),
        "the next chunk decides"
    );
    assert!(
        !spells_word(b"f;", b"f", false, true),
        "the last chunk decided"
    );
    assert!(!spells_word(b"ff();", b"f", true, true));
    assert!(!spells_word(b"$f", b"f", true, true));
    assert!(!spells_word("éf".as_bytes(), b"f", true, true));
    assert!(!spells_word(b"", b"f", true, true));
}

/// A file is searched whole, in chunks: a name across a chunk boundary is
/// found, and one that only starts a longer word there is not.
#[test]
fn files_are_searched_across_chunk_boundaries() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("big.ts");
    let pad = "x".repeat(MENTION_CHUNK_BYTES - 2);
    write(&file, &format!("{pad} target();\n"));
    assert_eq!(file_spells(&file, b"target"), Some(true));
    write(&file, &format!("{pad} targets();\n"));
    assert_eq!(file_spells(&file, b"target"), Some(false));
    // A word ending the first chunk is decided there, not again from the
    // carried tail that starts with it.
    let pad = "x".repeat(MENTION_CHUNK_BYTES - 8);
    write(&file, &format!("{pad}atarget;\n"));
    assert_eq!(file_spells(&file, b"target"), Some(false));
    // A file gone since it was listed spells nothing; an unreadable one is
    // undecided.
    assert_eq!(
        file_spells(&dir.path().join("missing.ts"), b"target"),
        Some(false)
    );
    assert_eq!(file_spells(dir.path(), b"target"), None);
}
