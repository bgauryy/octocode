//! Cuts of source text a result shows (a long line's window around its hit,
//! a value's span) that never show part of a secret. A cut taken from the
//! raw text can split a secret so that no pattern matches the part shown (a
//! token without its prefix, a value without its key), and a later scan of
//! the shown text cannot redact it. So a cut is taken from its source
//! sanitized whole ([`sanitize_then_cut`]); a cut made before sanitizing (the
//! engine's value spans) is checked against the sanitized source instead
//! ([`exposes`]).

/// The cut `cut` takes of `text` sanitized whole by `sanitize`. `cut` gets
/// the sanitized text and whether sanitizing changed it: its positions then
/// differ from the raw text's, so a window anchored on a raw offset must be
/// placed again.
pub(crate) fn sanitize_then_cut<E>(
    text: &str,
    sanitize: impl FnOnce(&str) -> Result<String, E>,
    cut: impl FnOnce(&str, bool) -> String,
) -> Result<String, E> {
    let clean = sanitize(text)?;
    let changed = clean != text;
    Ok(cut(&clean, changed))
}

/// [`sanitize_then_cut`] with the content scanner every tool's output passes
/// through, for a cut made where no caller-supplied scanner reaches; a
/// scanner failure shows the redaction placeholder.
pub(crate) fn scan_then_cut(text: &str, cut: impl FnOnce(&str, bool) -> String) -> String {
    sanitize_then_cut(
        text,
        |raw| {
            octocode_engine::security::sanitizer::sanitize_content(raw, None)
                .map(|result| result.content)
        },
        cut,
    )
    .unwrap_or_else(|_| octocode_engine::security::SANITIZER_FAILURE_PLACEHOLDER.to_owned())
}

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

    /// A window cut after sanitizing shows the redaction, never the part of
    /// the token the window holds; a cut made first would show it bare.
    #[test]
    fn a_cut_after_sanitizing_never_shows_part_of_a_secret() {
        let line = format!("let token = \"{SECRET}\"; needle");
        let tail = |text: &str, _changed: bool| text.chars().skip(16).collect::<String>();
        let raw_cut = tail(&line, false);
        assert!(raw_cut.contains(&SECRET[8..28]), "the raw cut splits it");
        let shown = scan_then_cut(&line, tail);
        assert!(!shown.contains(&SECRET[8..28]), "{shown}");
        assert!(shown.contains("needle"), "{shown}");
        // A line with nothing to redact is cut unchanged.
        let plain = scan_then_cut("plain needle line", |text, changed| {
            assert!(!changed);
            text.to_owned()
        });
        assert_eq!(plain, "plain needle line");
    }

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
