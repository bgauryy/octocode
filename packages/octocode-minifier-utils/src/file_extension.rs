/// Extract the file extension from a path, handling dotfiles correctly.
/// Mirrors the TypeScript `getExtension()` in octocode-minifier.
pub fn get_extension_internal(
    file_path: &str,
    lowercase: bool,
    fallback: &str,
) -> String {
    let basename = file_path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(file_path);

    let parts: Vec<&str> = basename.splitn(3, '.').collect();

    match parts.as_slice() {
        // No dot → no extension
        [_] => fallback.to_owned(),
        // Dotfile: ".gitignore" → parts = ["", "gitignore"]
        ["", ext] => {
            if lowercase {
                ext.to_lowercase()
            } else {
                ext.to_string()
            }
        }
        // Normal file: collect everything after the last dot
        _ => {
            let last_dot = basename.rfind('.').unwrap();
            let ext = &basename[last_dot + 1..];
            if lowercase {
                ext.to_lowercase()
            } else {
                ext.to_string()
            }
        }
    }
}
