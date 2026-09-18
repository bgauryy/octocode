/// Canonical JavaScript/TypeScript family extensions (lowercase, no leading
/// dot). Single source of truth for every JS/TS dispatch in the crate — native
/// oxc symbols/references, the oxc minify fast path, and the
/// `getSupportedJsTsExtensions` napi export all read this, so the set never
/// drifts between modules.
pub const JS_TS_EXTENSIONS: &[&str] = &["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];

/// True when `ext` (lowercase, no leading dot) is a JS/TS family extension.
pub fn is_js_ts_extension(ext: &str) -> bool {
    JS_TS_EXTENSIONS.contains(&ext)
}

/// Extract the file extension from a path, handling dotfiles correctly.
pub fn get_extension_internal(file_path: &str, lowercase: bool, fallback: &str) -> String {
    let basename = file_path.rsplit(['/', '\\']).next().unwrap_or(file_path);

    let ext = if let Some(dotfile_ext) = basename.strip_prefix('.') {
        if dotfile_ext.contains('.') {
            basename
                .rsplit_once('.')
                .map(|(_, ext)| ext)
                .unwrap_or(fallback)
        } else {
            dotfile_ext
        }
    } else {
        basename
            .rsplit_once('.')
            .map(|(_, ext)| ext)
            .unwrap_or(fallback)
    };

    if lowercase {
        ext.to_lowercase()
    } else {
        ext.to_owned()
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

    #[test]
    fn last_dot_wins_for_multi_dot_dotfiles() {
        assert_eq!(get_extension_internal(".env.local", false, ""), "local");
    }

    #[test]
    fn windows_path_basename_is_supported() {
        assert_eq!(get_extension_internal(r"C:\tmp\Foo.TS", true, ""), "ts");
    }
}
