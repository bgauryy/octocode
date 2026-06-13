use crate::config::{minify_config, indentation_sensitive_names, FileTypeConfig};
use crate::file_extension::get_extension_internal;
use crate::types::MinifyResult;
use crate::strategies::{
    minify_conservative, minify_aggressive,
    minify_general_core, minify_markdown_core,
    minify_css_quality,
    minify_html_core, minify_html_quality,
    minify_javascript_core, minify_js_oxc,
    minify_json_core_inner,
};

pub(crate) const MAX_SIZE: usize = 1024 * 1024; // 1 MB guard, shared by all FFI content entry points

pub fn get_file_config(file_path: &str) -> Option<&'static FileTypeConfig> {
    let ext = get_extension_internal(file_path, true, "txt");
    let basename = file_path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(file_path)
        .to_lowercase();

    if indentation_sensitive_names().contains(basename.as_str()) {
        return minify_config().get("sh"); // hash comments, conservative
    }

    minify_config().get(ext.as_str())
}

pub fn comment_groups(cfg: &FileTypeConfig) -> Vec<&'static str> {
    cfg.comments.map(|c| c.to_vec()).unwrap_or_default()
}

/// Synchronous full minification — mirrors TS `minifyContentSync`.
pub fn minify_content_sync_inner(content: &str, file_path: &str) -> String {
    if content.len() > MAX_SIZE { return content.to_owned(); }
    dispatch(content, file_path, false)
}

/// Full minification returning MinifyResult — mirrors TS `minifyContent` (async in TS).
/// In Rust all operations are sync; callers may wrap in Promise.resolve() on the JS side.
pub fn minify_content_result_inner(content: &str, file_path: &str) -> MinifyResult {
    let content_size = content.len();
    if content_size > MAX_SIZE {
        return MinifyResult::fail(
            content.to_owned(),
            format!("File too large: {:.2}MB exceeds 1MB limit", content_size as f64 / 1_048_576.0),
        );
    }

    let Some(cfg) = get_file_config(file_path) else {
        let out = minify_general_core(content);
        return MinifyResult::ok(out, "general");
    };
    let ext  = get_extension_internal(file_path, true, "txt");
    let grps = comment_groups(cfg);

    let (out, strategy) = match cfg.strategy {
        "terser" | "conservative" => {
            let s = if matches!(ext.as_str(), "ts"|"tsx"|"js"|"jsx"|"mjs"|"cjs") {
                // OXC: full compression with mangle for the minify path
                minify_js_oxc(content, file_path, true)
                    .unwrap_or_else(|| minify_javascript_core(content))
            } else {
                minify_conservative(content, Some(&grps))
            };
            (s, cfg.strategy)
        }
        "aggressive" => {
            let s = if matches!(ext.as_str(), "css"|"less"|"scss") {
                minify_css_quality(content)          // lightningcss
            } else if matches!(ext.as_str(), "html"|"htm") {
                minify_html_quality(content)         // minify-html
            } else if matches!(ext.as_str(), "xml"|"svg") {
                minify_html_core(content)
            } else {
                minify_aggressive(content, Some(&grps))
            };
            (s, "aggressive")
        }
        "json" => {
            let (s, _) = minify_json_core_inner(content);
            (s, "json")
        }
        "markdown" => (minify_markdown_core(content), "markdown"),
        _           => (minify_general_core(content), "general"),
    };

    MinifyResult::ok(out, strategy)
}

// Shared dispatch (sync path without MinifyResult overhead)
fn dispatch(content: &str, file_path: &str, _high_quality: bool) -> String {
    let Some(cfg) = get_file_config(file_path) else {
        return minify_general_core(content);
    };
    let ext  = get_extension_internal(file_path, true, "txt");
    let grps = comment_groups(cfg);

    match cfg.strategy {
        "terser" | "conservative" => {
            if matches!(ext.as_str(), "ts"|"tsx"|"js"|"jsx"|"mjs"|"cjs") {
                minify_js_oxc(content, file_path, true)
                    .unwrap_or_else(|| minify_javascript_core(content))
            } else {
                minify_conservative(content, Some(&grps))
            }
        }
        "aggressive" => {
            if matches!(ext.as_str(), "css"|"less"|"scss") {
                minify_css_quality(content)
            } else if matches!(ext.as_str(), "html"|"htm") {
                minify_html_quality(content)
            } else if matches!(ext.as_str(), "xml"|"svg") {
                minify_html_core(content)
            } else {
                minify_aggressive(content, Some(&grps))
            }
        }
        "json"     => { let (s, _) = minify_json_core_inner(content); s }
        "markdown" => minify_markdown_core(content),
        _          => minify_general_core(content),
    }
}
