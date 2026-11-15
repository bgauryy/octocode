//! The entry text shared by local and remote directory listings.
/// One listed entry: its `name`, `/` after a directory, then ` (<fields>)`
/// when it has `fields`. The fields never contain ` (`, so the last ` (` of an
/// entry ending in `)` opens them, whatever the name holds.
pub(crate) fn entry_text(name: &str, is_dir: bool, fields: &[String]) -> String {
    let mut text = name.to_owned();
    if is_dir {
        text.push('/');
    }
    if !fields.is_empty() {
        text.push_str(&format!(" ({})", fields.join(", ")));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::entry_text;

    #[test]
    fn an_entry_names_a_directory_with_a_slash_and_lists_its_fields() {
        assert_eq!(entry_text("a.rs", false, &[]), "a.rs");
        assert_eq!(entry_text("src", true, &[]), "src/");
        let fields = ["12".to_owned(), "symlink".to_owned()];
        assert_eq!(entry_text("a.rs", false, &fields), "a.rs (12, symlink)");
        assert_eq!(entry_text("src", true, &fields[..1]), "src/ (12)");
    }
}
