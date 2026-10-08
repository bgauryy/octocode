use crate::minify::config::{FileTypeConfig, indentation_sensitive_names, minify_config};
use crate::text::file_extension::extension_of;

pub(crate) const MAX_SIZE: usize = 1024 * 1024; // 1 MB content-view guard

pub fn get_file_config(file_path: &str) -> Option<&'static FileTypeConfig> {
    let ext = extension_of(file_path, true, "txt");
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
