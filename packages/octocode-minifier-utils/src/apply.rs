use crate::config::{minify_config, indentation_sensitive_names};
use crate::file_extension::get_extension_internal;
use crate::minifier::minify_content_sync_inner;
use crate::strategies::{
    minify_json_readable_inner, minify_markdown_core,
    minify_general_core, minify_code_core,
};
use crate::comment_remover::remove_comments;

/// Full minification — return minified if shorter, else original.
pub fn apply_minification_inner(content: &str, file_path: &str) -> String {
    let minified = std::panic::catch_unwind(|| minify_content_sync_inner(content, file_path))
        .unwrap_or_else(|_| content.to_owned());
    if minified.len() < content.len() { minified } else { content.to_owned() }
}

/// Content-view minification — agent-readable, preserves indentation.
/// Pipeline mirrors TS `applyContentViewMinification`.
pub fn apply_content_view_minification_inner(content: &str, file_path: &str) -> String {
    let result = std::panic::catch_unwind(|| {
        let ext = get_extension_internal(file_path, true, "txt");
        let basename = file_path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(file_path)
            .to_lowercase();

        let cfg = if indentation_sensitive_names().contains(basename.as_str()) {
            minify_config().get("sh")
        } else {
            minify_config().get(ext.as_str())
        };

        if matches!(ext.as_str(), "json" | "jsonc" | "json5") {
            let (out, _) = minify_json_readable_inner(content);
            return out;
        }

        if cfg.map(|c| c.strategy) == Some("markdown") {
            return minify_markdown_core(content);
        }

        let stripped = if let Some(c) = cfg {
            if let Some(groups) = c.comments {
                remove_comments(content, groups)
            } else {
                content.to_owned()
            }
        } else {
            content.to_owned()
        };

        match cfg.map(|c| c.strategy) {
            None | Some("general") => minify_general_core(&stripped),
            _                      => minify_code_core(&stripped),
        }
    }).unwrap_or_else(|_| content.to_owned());

    if result.len() < content.len() { result } else { content.to_owned() }
}
