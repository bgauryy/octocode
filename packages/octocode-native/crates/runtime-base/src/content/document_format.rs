//! The document formats the `minify:"symbols"` view outlines by their own
//! structure (Markdown headings, data keys) rather than by declarations: no
//! grammar's declaration extractor reads them.

/// A document format, by the extension of its path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentFormat {
    Markdown,
    Json,
    Yaml,
    Toml,
}

impl DocumentFormat {
    /// The format `path`'s extension names, in any case.
    pub fn of(path: &str) -> Option<Self> {
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
mod tests;
