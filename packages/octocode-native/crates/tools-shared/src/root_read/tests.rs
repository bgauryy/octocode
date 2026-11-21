use super::{entry_fields, entry_text};

#[test]
fn an_entry_names_a_directory_with_a_slash_and_lists_its_fields() {
    assert_eq!(entry_text("a.rs", false, &[]), "a.rs");
    assert_eq!(entry_text("src", true, &[]), "src/");
    let fields = ["12".to_owned(), "symlink".to_owned()];
    assert_eq!(entry_text("a.rs", false, &fields), "a.rs (12, symlink)");
    assert_eq!(entry_text("src", true, &fields[..1]), "src/ (12)");
}

/// Reading an entry back is the inverse of writing it, whatever the name
/// holds; an entry without fields has none to split off.
#[test]
fn an_entry_reads_back_as_its_name_and_fields() {
    let fields = ["12".to_owned(), "link -> a/b".to_owned()];
    assert_eq!(entry_fields(&entry_text("a.rs", false, &[])), None);
    assert_eq!(entry_fields(&entry_text("src", true, &[])), None);
    for name in ["a.rs", "notes (old)", "deep/a (1) (2).txt"] {
        for is_dir in [false, true] {
            let bare = entry_text(name, is_dir, &[]);
            let entry = entry_text(name, is_dir, &fields);
            assert_eq!(
                entry_fields(&entry),
                Some((bare.as_str(), "12, link -> a/b"))
            );
        }
    }
}
