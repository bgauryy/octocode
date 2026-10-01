//! Line-anchored hits for repo-scoped code searches. GitHub's code index
//! returns at most a few unnumbered fragments per file, so the top files are
//! read (core API quota, through the contents cache) and every line holding a
//! keyword is listed as `"<line>\t<text>"` (the numbered-line form), the
//! way `grep -n` would.
use crate::providers::github::{
    CredentialResolver, GitHubProvider, ProviderError, ProviderErrorKind, RequestContext,
};
use crate::security::scan::ContentScan;
use std::path::Path;

/// Files a repo-scoped page resolves to line numbers.
pub(super) const MAX_RESOLVED_FILES: usize = 5;
/// Hit lines listed per file; a file with more names the rest by count.
pub(super) const MAX_LINES_PER_FILE: usize = 20;
/// Characters kept per hit line (a window around the first keyword).
const MAX_LINE_CHARS: usize = 200;
/// Blobs larger than this keep their index fragments.
const MAX_SCAN_BYTES: usize = 2 * 1024 * 1024;

/// One file's hits at a resolved commit.
#[derive(Debug, PartialEq)]
pub(super) enum FileHits {
    /// Every line holding a keyword (capped at [`MAX_LINES_PER_FILE`]).
    Lines {
        lines: Vec<String>,
        first: u32,
        last: u32,
        total: usize,
        line_count: usize,
    },
    /// The blob was read but no line holds a keyword (index lag or
    /// tokenization differences): keep the index fragments.
    Unmatched,
    /// The path does not exist at the resolved ref.
    Missing,
    /// The blob could not be read or scanned (binary, too large, transient).
    Unavailable,
}

/// Resolve `reference` (`None`: the default branch HEAD) to a commit SHA.
pub(super) async fn resolve_commit<
    R: CredentialResolver,
    C: crate::providers::github::ConditionalCache,
>(
    provider: &GitHubProvider<R, C>,
    owner: &str,
    repo: &str,
    reference: Option<&str>,
    context: &RequestContext,
) -> Result<String, ProviderError> {
    provider
        .resolve_reference(owner, repo, reference, false, context)
        .await
}

/// Read each path at `sha` concurrently and list its keyword lines.
/// Cancellation and auth/rate failures propagate as `Unavailable` for the
/// file, except cancellation, which aborts.
pub(super) async fn resolve_files<
    R: CredentialResolver,
    C: crate::providers::github::ConditionalCache,
>(
    provider: &GitHubProvider<R, C>,
    owner: &str,
    repo: &str,
    sha: &str,
    paths: &[String],
    keywords: &[String],
    context: &RequestContext,
    security: &impl ContentScan,
) -> Result<Vec<FileHits>, ProviderError> {
    let reads = paths.iter().map(|path| async move {
        provider
            .get_file_content(
                &crate::providers::github::ContentRequest {
                    owner: owner.to_owned(),
                    repo: repo.to_owned(),
                    path: path.clone(),
                    reference: Some(sha.to_owned()),
                    force_refresh: false,
                    session_id: None,
                },
                context,
            )
            .await
    });
    let fetched = futures_util::future::join_all(reads).await;
    let mut out = Vec::with_capacity(paths.len());
    for (path, result) in paths.iter().zip(fetched) {
        out.push(match result {
            Ok(content) => scan(&content.bytes, path, keywords, security),
            Err(error) if error.kind == ProviderErrorKind::Cancelled => return Err(error),
            Err(error)
                if error.kind == ProviderErrorKind::NotFound || error.status == Some(404) =>
            {
                FileHits::Missing
            }
            Err(_) => FileHits::Unavailable,
        });
    }
    Ok(out)
}

/// Case-insensitive keyword lines of one blob, numbered from 1.
pub(super) fn scan(
    bytes: &[u8],
    path: &str,
    keywords: &[String],
    security: &impl ContentScan,
) -> FileHits {
    if bytes.len() > MAX_SCAN_BYTES || bytes.iter().take(8192).any(|byte| *byte == 0) {
        return FileHits::Unavailable;
    }
    let text = String::from_utf8_lossy(bytes);
    let needles: Vec<String> = keywords
        .iter()
        .map(|keyword| keyword.trim().to_lowercase())
        .filter(|keyword| !keyword.is_empty())
        .collect();
    if needles.is_empty() {
        return FileHits::Unmatched;
    }
    let key_blocks = crate::security::private_key_block_line_ranges(&text);
    let mut lines = Vec::new();
    let mut first = 0;
    let mut last = 0;
    let mut total = 0;
    let mut line_count = 0;
    for (index, line) in text.lines().enumerate() {
        line_count = index + 1;
        let lower = line.to_lowercase();
        let Some(at) = needles
            .iter()
            .filter_map(|needle| lower.find(needle.as_str()))
            .min()
        else {
            continue;
        };
        let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
        total += 1;
        if first == 0 {
            first = number;
        }
        last = number;
        if lines.len() >= MAX_LINES_PER_FILE {
            continue;
        }
        let in_key_block = key_blocks
            .iter()
            .any(|&(start, end)| (start..=end).contains(&number));
        let shown = if in_key_block {
            crate::security::key_fragment_placeholder()
        } else {
            let window = clip(line, lower_to_char_index(line, &lower, at));
            security
                .sanitize(&window, Path::new(path))
                .map(|(clean, _)| clean)
                .unwrap_or_else(|_| "[REDACTED]".to_owned())
        };
        lines.push(format!(
            "{number}{}{shown}",
            crate::runtime::numbered::SEPARATOR
        ));
    }
    if total == 0 {
        return FileHits::Unmatched;
    }
    FileHits::Lines {
        lines,
        first,
        last,
        total,
        line_count,
    }
}

/// Char index in `line` of byte offset `at` in its lowercase form (lowercasing
/// can change byte lengths, so map through char counts).
fn lower_to_char_index(line: &str, lower: &str, at: usize) -> usize {
    if line.len() == lower.len() {
        return line[..at.min(line.len())].chars().count();
    }
    lower
        .get(..at)
        .map_or(0, |prefix| prefix.chars().count())
        .min(line.chars().count())
}

/// The line itself when short; otherwise a window starting a little before
/// the first keyword, marked with `…`.
fn clip(line: &str, at_char: usize) -> String {
    let chars = line.chars().count();
    if chars <= MAX_LINE_CHARS {
        return line.to_owned();
    }
    let start = at_char.saturating_sub(40).min(chars - MAX_LINE_CHARS);
    let body: String = line.chars().skip(start).take(MAX_LINE_CHARS).collect();
    let prefix = if start > 0 { "…" } else { "" };
    let suffix = if start + MAX_LINE_CHARS < chars {
        "…"
    } else {
        ""
    };
    format!("{prefix}{body}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Passthrough;
    impl ContentScan for Passthrough {
        fn sanitize(
            &self,
            text: &str,
            _: &Path,
        ) -> Result<(String, Vec<String>), (String, String)> {
            Ok((text.to_owned(), vec![]))
        }
    }

    #[test]
    fn every_keyword_line_is_numbered_case_insensitively() {
        let text = "import x\nawait Wrap_App(app)\nother\n    await wrap_app(s)\n";
        let hits = scan(text.as_bytes(), "a.py", &["wrap_app".into()], &Passthrough);
        assert_eq!(
            hits,
            FileHits::Lines {
                lines: vec![
                    "2\tawait Wrap_App(app)".into(),
                    "4\t    await wrap_app(s)".into()
                ],
                first: 2,
                last: 4,
                total: 2,
                line_count: 4,
            }
        );
        assert_eq!(
            scan(text.as_bytes(), "a.py", &["absent".into()], &Passthrough),
            FileHits::Unmatched
        );
        assert_eq!(
            scan(
                b"bin\0ary needle",
                "a.bin",
                &["needle".into()],
                &Passthrough
            ),
            FileHits::Unavailable
        );
    }

    #[test]
    fn long_lines_keep_a_window_around_the_keyword() {
        let line = format!("{}needle{}", "a".repeat(500), "b".repeat(500));
        let FileHits::Lines { lines, .. } =
            scan(line.as_bytes(), "a.js", &["needle".into()], &Passthrough)
        else {
            panic!("hit");
        };
        assert!(lines[0].starts_with("1\t…"), "{}", lines[0]);
        assert!(lines[0].contains("needle"));
        assert!(lines[0].chars().count() <= MAX_LINE_CHARS + 4);
    }

    #[test]
    fn many_hits_are_capped_but_counted() {
        let text = "needle\n".repeat(30);
        let FileHits::Lines {
            lines, total, last, ..
        } = scan(text.as_bytes(), "a.rs", &["needle".into()], &Passthrough)
        else {
            panic!("hit");
        };
        assert_eq!(lines.len(), MAX_LINES_PER_FILE);
        assert_eq!(total, 30);
        assert_eq!(last, 30);
    }
}
