//! A literal search that misses and reads as a regex runs as one: an agent
//! writing `a|b` means alternatives, and a silent empty row reads as absence.
use super::*;

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("fixture");
    fs::write(root.path().join("a.rs"), "fn alpha() {}\nfn beta() {}\n").expect("a");
    root
}

#[test]
fn a_literal_miss_that_reads_as_a_regex_finds_its_hits_and_says_so() {
    let root = fixture();
    let found = search_root(
        root.path(),
        serde_json::json!({"matchString": ["alpha|beta"]}),
    );
    let row = found.to_row().expect("row");
    assert_eq!(row["matchCount"], 2, "{row}");
    let notes = row["hints"].to_string();
    assert!(notes.contains("regexEngine"), "disclosed once: {row}");
    assert_eq!(notes.matches("regexEngine").count(), 1, "{row}");
}

#[test]
fn a_literal_hit_stays_literal_even_when_it_reads_as_a_regex() {
    let root = tempfile::tempdir().expect("fixture");
    fs::write(root.path().join("a.rs"), "a|b\nab\n").expect("a");
    let row = search_root(root.path(), serde_json::json!({"matchString": ["a|b"]}))
        .to_row()
        .expect("row");
    assert_eq!(row["matchCount"], 1, "the literal line only: {row}");
    assert!(!row.to_string().contains("regexEngine"), "{row}");
}

#[test]
fn plain_words_and_broken_regexes_stay_empty() {
    let root = fixture();
    for term in ["gamma", "fn gamma(", "gamma|delta", "fn.alpha"] {
        let row = search_root(root.path(), serde_json::json!({"matchString": [term]}))
            .to_row()
            .expect("row");
        assert_eq!(row["matchCount"], 0, "{term}: {row}");
        assert!(!row.to_string().contains("regexEngine"), "{term}: {row}");
    }
}

#[test]
fn every_page_of_a_regex_reading_keeps_the_regex() {
    let root = tempfile::tempdir().expect("fixture");
    let body: String = (0..40).map(|n| format!("alpha {n}\nbeta {n}\n")).collect();
    fs::write(root.path().join("a.rs"), body).expect("a");
    let first = search_root(
        root.path(),
        serde_json::json!({"matchString": ["alpha|beta"], "limit": 5}),
    );
    let row = first.to_row().expect("row");
    assert_eq!(row["matchCount"], 80, "{row}");
    assert!(row["next"]["nextPage"].is_object(), "{row}");
}
