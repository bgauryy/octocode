use super::entry_text;

#[test]
fn an_entry_names_a_directory_with_a_slash_and_lists_its_fields() {
    assert_eq!(entry_text("a.rs", false, &[]), "a.rs");
    assert_eq!(entry_text("src", true, &[]), "src/");
    let fields = ["12".to_owned(), "symlink".to_owned()];
    assert_eq!(entry_text("a.rs", false, &fields), "a.rs (12, symlink)");
    assert_eq!(entry_text("src", true, &fields[..1]), "src/ (12)");
}
