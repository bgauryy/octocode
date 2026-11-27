use super::identifier_words;

#[test]
fn identifier_words_and_unicode_are_searchable() {
    assert_eq!(
        identifier_words("HTTPServer retryConnection snake_case café"),
        [
            "http",
            "server",
            "retry",
            "connection",
            "snake",
            "case",
            "café"
        ]
    );
}

#[test]
fn paths_and_qualified_names_split_at_every_separator() {
    assert_eq!(
        identifier_words("src/pool.rs#OriginPool.read crate::io::read_to-end"),
        [
            "src", "pool", "rs", "origin", "pool", "read", "crate", "io", "read", "to", "end"
        ]
    );
}
