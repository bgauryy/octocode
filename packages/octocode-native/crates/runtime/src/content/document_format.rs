//! The document formats the `minify:"symbols"` view outlines by their own
//! structure (Markdown headings, data keys) rather than by declarations: no
//! grammar's declaration extractor reads them.

/// A document format, by the extension of its path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DocumentFormat {
    Markdown,
    Json,
    Yaml,
    Toml,
}

impl DocumentFormat {
    /// The format `path`'s extension names, in any case.
    pub(crate) fn of(path: &str) -> Option<Self> {
        let extension = std::path::Path::new(path)
            .extension()?
            .to_str()?
            .to_ascii_lowercase();
        Some(match extension.as_str() {
            "md" | "markdown" | "mdx" => Self::Markdown,
            "json" | "jsonc" | "json5" => Self::Json,
            "yaml" | "yml" => Self::Yaml,
            "toml" => Self::Toml,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::DocumentFormat;

    #[test]
    fn a_format_is_read_from_the_extension_in_any_case() {
        for (path, format) in [
            ("README.MD", Some(DocumentFormat::Markdown)),
            ("a/package.json", Some(DocumentFormat::Json)),
            ("x.yml", Some(DocumentFormat::Yaml)),
            ("Cargo.toml", Some(DocumentFormat::Toml)),
            ("lib.rs", None),
            ("noext", None),
        ] {
            assert_eq!(DocumentFormat::of(path), format, "{path}");
        }
    }
}
