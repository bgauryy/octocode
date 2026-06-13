/// Extract the file extension from a path, handling dotfiles correctly.
/// Mirrors the TypeScript `getExtension()` in octocode-minifier.
pub fn get_extension_internal(file_path: &str, lowercase: bool, fallback: &str) -> String {
    let basename = file_path.rsplit(['/', '\\']).next().unwrap_or(file_path);

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
        // Normal file: collect everything after the last dot.
        // This arm only matches when basename contains a dot, but stay
        // panic-free at the boundary anyway.
        _ => {
            let Some(last_dot) = basename.rfind('.') else {
                return fallback.to_owned();
            };
            let ext = &basename[last_dot + 1..];
            if lowercase {
                ext.to_lowercase()
            } else {
                ext.to_string()
            }
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_returned_when_path_has_dot() {
        assert_eq!(get_extension_internal("foo.ts", false, ""), "ts");
    }

    #[test]
    fn extension_lowercased_when_lowercase_requested() {
        assert_eq!(get_extension_internal("Foo.TS", true, ""), "ts");
    }

    #[test]
    fn dotfile_name_treated_as_extension() {
        assert_eq!(get_extension_internal(".gitignore", true, ""), "gitignore");
    }

    #[test]
    fn fallback_returned_when_no_extension() {
        assert_eq!(get_extension_internal("Makefile", false, "txt"), "txt");
    }

    #[test]
    fn last_dot_wins_for_multi_dot_names() {
        assert_eq!(get_extension_internal("archive.tar.gz", false, ""), "gz");
    }
}
