//! Check cuts of source text against the fully sanitized source. A cut taken
//! before sanitizing can split a secret so its prefix or key no longer matches
//! a redaction pattern. [`exposes`] detects those partial disclosures.

/// A shown line without the clip markers at its ends: a leading `…`, a
/// trailing `…` or `...` truncation suffix.
pub(crate) fn strip_clip_markers(line: &str) -> &str {
    let line = line.strip_prefix('…').unwrap_or(line);
    let line = line.strip_suffix("...").unwrap_or(line);
    line.strip_suffix('…').unwrap_or(line)
}

/// Whether `shown`, a cut of a source made before sanitizing, holds text the
/// source sanitized whole (`sanitized`) does not: a shown line, without its
/// clip markers, that overlapped a redaction (part of a secret the cut split).
pub(crate) fn exposes(shown: &str, sanitized: &str) -> bool {
    shown.lines().any(|line| {
        let core = strip_clip_markers(line);
        !core.is_empty() && !sanitized.contains(core)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "ghp_Zq7Zq7Zq7Zq7Zq7Zq7Zq7Zq7Zq7Zq7Zq7Zq7";

    /// A pre-sanitizing cut that holds part of a redacted secret is
    /// exposed; one holding only text the sanitized source keeps is not.
    #[test]
    fn a_cut_made_first_is_exposed_when_it_overlaps_a_redaction() {
        let source = format!("let token = \"{SECRET}\";");
        let sanitized = octocode_engine::security::sanitizer::sanitize_content(&source, None)
            .expect("sanitized")
            .content;
        assert_ne!(sanitized, source);
        assert!(exposes(&format!("…{}…", &SECRET[6..30]), &sanitized));
        assert!(!exposes("…let token = \"…", &sanitized));
        assert!(!exposes("", &sanitized));
    }
}
