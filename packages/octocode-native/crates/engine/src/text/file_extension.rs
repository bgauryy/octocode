mod c_header;

/// Canonical JavaScript/TypeScript family extensions (lowercase, no leading
/// dot). Single source of truth for every JS/TS dispatch in the crate — native
/// oxc symbols/references and the JS/TS content-view minifier both read this,
/// so the set never drifts between modules.
pub const JS_TS_EXTENSIONS: &[&str] = &["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];

/// The JavaScript (non-TypeScript) members of [`JS_TS_EXTENSIONS`]: the
/// extensions the JavaScript grammar parses.
pub const JS_EXTENSIONS: &[&str] = &["js", "jsx", "mjs", "cjs"];

/// The final component of `path`: after the last path separator of the
/// platform (`/` everywhere, `\\` too on Windows). On Unix `\\` is a legal
/// file-name byte, so `f\\g.rs` is one name.
pub fn basename(path: &str) -> &str {
    path.rsplit(std::path::is_separator).next().unwrap_or(path)
}

/// The extension of the path's basename ([`basename`]), the
/// one extension helper for the crate. A dotfile names its own extension
/// (`.bashrc` → `bashrc`); a dot in a directory name is never read; a name
/// without a dot gives `fallback`. `lowercase` folds case so `.TS` matches
/// `ts`.
pub fn extension_of(file_path: &str, lowercase: bool, fallback: &str) -> String {
    let basename = basename(file_path);

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
pub(crate) fn is_flow_source(content: &str, ext: &str) -> bool {
    if !JS_EXTENSIONS.contains(&ext) {
        return false;
    }
    let head_end = content.floor_char_boundary(FLOW_PRAGMA_WINDOW);
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
/// (see [`is_flow_source`]), the C++ or CUDA grammar for a `.h` header of
/// that dialect (see [`c_header`]), otherwise `ext` unchanged.
pub(crate) fn grammar_extension<'a>(content: &str, ext: &'a str) -> &'a str {
    if is_flow_source(content, ext) {
        "tsx"
    } else if ext == "h" {
        c_header::header_grammar(content).unwrap_or(ext)
    } else {
        ext
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests;
