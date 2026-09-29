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

/// Bytes of a file's head searched for the `@flow` pragma.
const FLOW_PRAGMA_WINDOW: usize = 4096;

/// `@flow` as its own word (`@flow`, `@flow strict`), not the prefix of a
/// package name or another tag such as `@flowjs/flow.js`.
fn has_flow_pragma(head: &str) -> bool {
    head.match_indices("@flow").any(|(start, tag)| {
        head[start + tag.len()..]
            .chars()
            .next()
            .is_none_or(|next| !(next.is_alphanumeric() || matches!(next, '_' | '-' | '/')))
    })
}

/// True when a JavaScript-family source (`js`/`jsx`/`mjs`/`cjs`) carries Flow
/// type syntax: an `@flow` pragma in its head, or a Flow-only statement
/// (`import type`, `import typeof`, `export type`, `opaque type`) at the start
/// of a line. The JS grammars reject that syntax, so such a file parses into
/// garbage (keywords recovered as declarations). Flow's annotation syntax is
/// close to TypeScript's, so callers parse it with a TSX grammar instead.
pub fn is_flow_source(content: &str, ext: &str) -> bool {
    if !matches!(ext, "js" | "jsx" | "mjs" | "cjs") {
        return false;
    }
    let mut head_end = content.len().min(FLOW_PRAGMA_WINDOW);
    while !content.is_char_boundary(head_end) {
        head_end -= 1;
    }
    if has_flow_pragma(&content[..head_end]) {
        return true;
    }
    content.lines().any(|line| {
        let line = line.trim_start();
        [
            "import type ",
            "import typeof ",
            "export type ",
            "opaque type ",
        ]
        .iter()
        .any(|prefix| line.starts_with(prefix))
    })
}

/// Grammar extension to parse `ext` with: `tsx` for Flow-typed JavaScript
/// (see [`is_flow_source`]), otherwise `ext` unchanged.
pub fn grammar_extension<'a>(content: &str, ext: &'a str) -> &'a str {
    if is_flow_source(content, ext) {
        "tsx"
    } else {
        ext
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flow_pragma_must_be_a_word_not_a_package_prefix() {
        assert!(is_flow_source("// @flow strict\nconst a = 1;\n", "js"));
        assert!(is_flow_source("/* @flow */ const a = 1;\n", "cjs"));
        assert!(!is_flow_source(
            "// see @flowjs/flow.js\nconst a = require('x');\n",
            "cjs"
        ));
        assert!(!is_flow_source(
            "// @flow-typed stubs\nconst a = 1;\n",
            "js"
        ));
    }

    #[test]
    fn flow_sources_are_detected_by_pragma_or_flow_only_statements() {
        assert!(is_flow_source("/**\n * @flow\n */\nconst a = 1;\n", "js"));
        assert!(is_flow_source("import type {A} from 'a';\n", "jsx"));
        assert!(is_flow_source(
            "const a = 1;\nexport type B = number;\n",
            "mjs"
        ));
        assert!(!is_flow_source("const a = 1;\n", "js"));
        assert!(!is_flow_source("// @flow\n", "ts"), "TS is never Flow");
        assert_eq!(grammar_extension("// @flow\n", "js"), "tsx");
        assert_eq!(grammar_extension("const a = 1;\n", "js"), "js");
    }

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
