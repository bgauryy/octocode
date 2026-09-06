use crate::types::GetExtensionOptions;
use napi_derive::napi;

/// Extract the file extension from a path (dotfile-aware).
/// Options control lowercasing and the configured default used when no extension exists.
#[napi(js_name = "getExtension")]
pub fn get_extension(file_path: String, options: Option<GetExtensionOptions>) -> String {
    let lowercase = options.as_ref().and_then(|o| o.lowercase).unwrap_or(false);
    let fallback = options
        .as_ref()
        .and_then(|o| o.fallback.as_deref())
        .unwrap_or("");
    crate::text::file_extension::get_extension_internal(&file_path, lowercase, fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_option_defaults_when_options_omitted() {
        assert_eq!(get_extension("foo.ts".into(), None), "ts");
        assert_eq!(get_extension("Makefile".into(), None), "");
    }

    #[test]
    fn honors_lowercase_and_default_options() {
        let opts = GetExtensionOptions {
            lowercase: Some(true),
            fallback: Some("txt".into()),
        };
        assert_eq!(get_extension("Foo.TS".into(), Some(opts)), "ts");
        let opts = GetExtensionOptions {
            lowercase: None,
            fallback: Some("txt".into()),
        };
        assert_eq!(get_extension("Makefile".into(), Some(opts)), "txt");
    }
}
