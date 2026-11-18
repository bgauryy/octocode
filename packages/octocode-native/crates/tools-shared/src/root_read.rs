//! The entry text shared by local and remote directory listings.
/// One listed entry: its `name`, `/` after a directory, then ` (<fields>)`
/// when it has `fields`. The fields never contain ` (`, so the last ` (` of an
/// entry ending in `)` opens them, whatever the name holds.
pub fn entry_text(name: &str, is_dir: bool, fields: &[String]) -> String {
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
mod tests;
