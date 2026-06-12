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
    std::panic::catch_unwind(|| {
        let ext = get_extension_internal(file_path, true, "txt");
        extract_by_ext(content, &ext)
    })
    .unwrap_or(None)
}

fn extract_by_ext(content: &str, ext: &str) -> Option<String> {
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
