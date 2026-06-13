pub mod extractor;
pub mod heuristic;
pub mod languages;
pub mod renderer;

use crate::file_extension::get_extension_internal;
use extractor::{extract, LangExtractConfig};

pub const SIGNATURES_ONLY_HINT: &str = concat!(
    "Signatures only — bodies and comments omitted; ",
    "the whole skeleton is returned in one response (never paginated). ",
    "Left gutter shows original line numbers; use startLine/endLine to read a body."
);

/// Extract a structural skeleton from `content`.
/// Returns `NNN| text` rendered string or `None`.
pub fn extract_signatures_inner(content: &str, file_path: &str) -> Option<String> {
    if content.len() > crate::minifier::MAX_SIZE { return None; }
    std::panic::catch_unwind(|| {
        let ext = get_extension_internal(file_path, true, "txt");
        extract_by_ext(content, &ext)
    })
    .unwrap_or(None)
}

/// Extensions where symbol extraction reliably produces output LARGER than
/// the source or adds no semantic value (config, data, and prose formats).
/// Extensions where symbol extraction has no semantic value:
/// data/config formats have key-value pairs, not code signatures;
/// prose/doc formats have section headings, not function declarations.
/// Code languages (Lua, Erlang, Clojure, VB) are intentionally excluded
/// even when their heuristic grows output — the skeleton is still useful.
const NO_SYMBOL_EXTS: &[&str] = &[
    // Data / config — no code signatures whatsoever
    "json", "jsonc", "json5",
    "yaml", "yml",
    "toml",
    "ini", "cfg", "conf", "config", "properties", "env",
    "csv", "tsv",
    "xml", "svg",
    // Prose / docs — no function declarations
    "md", "markdown",
    "rst",
    "txt", "log",
];

fn extract_by_ext(content: &str, ext: &str) -> Option<String> {
    // P0: never extract symbols for formats with no code signatures
    if NO_SYMBOL_EXTS.contains(&ext) { return None; }

    // ── tree-sitter path (top-10 languages) ─────────────────────────────────
    if let Some(entry) = languages::find_entry(ext) {
        let cfg = LangExtractConfig {
            language:      (entry.language_fn)(),
            body_query:    entry.body_query,
            comment_style: entry.comment_style,
        };
        // Try tree-sitter; fall back to heuristic on failure.
        if let Some(kept) = extract(content, &cfg) {
            return renderer::render_skeleton(&kept, entry.comment_style);
        }
        // Fall through to heuristic
    }

    // ── heuristic path (all other languages + TS fallback) ───────────────────
    let comment_style = comment_style_for(ext);
    let kept = heuristic::extract_heuristic(content, ext)?;
    renderer::render_skeleton(&kept, comment_style)
}

fn comment_style_for(ext: &str) -> &'static str {
    match ext {
        "py"|"rb"|"sh"|"bash"|"zsh"|"fish"|"coffee"|"r"|"nim"|"jl"
        |"pl"|"pm"|"ex"|"exs"|"cr"|"pp" => "hash",
        "hs"|"lhs"|"lua"|"erl"|"hrl" => "hash",
        "html"|"htm"|"vue"|"svelte"  => "html",
        "sql"|"tsql"|"plsql"         => "sql",
        "php"                        => "c-hash",
        _ => "c",
    }
}
