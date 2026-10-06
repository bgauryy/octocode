//! Line-anchored hits for repo-scoped code searches. GitHub's code index
//! returns at most a few unnumbered fragments per file, so every file of the
//! page is read (core API quota, through the contents cache) and its keyword
//! lines are listed as `"<line>\t<text>"` (the numbered-line form), the way
//! `grep -n` would: with several keywords, the lines holding all of them.
use crate::providers::github::{
    CredentialResolver, GitHubProvider, ProviderError, ProviderErrorKind, RequestContext,
};
use crate::security::scan::ContentScan;
use std::path::Path;

/// Hit lines listed per file; a file with more names the rest by count.
pub(super) const MAX_LINES_PER_FILE: usize = 20;
/// Characters kept per hit line (a window around the first keyword).
const MAX_LINE_CHARS: usize = 200;
/// Blobs larger than this keep their index fragments.
const MAX_SCAN_BYTES: usize = 2 * 1024 * 1024;

/// One file's hits at a resolved commit.
#[derive(Debug, PartialEq)]
pub(super) enum FileHits {
    /// The lines holding every keyword, else each keyword's first line
    /// (capped at [`MAX_LINES_PER_FILE`]); `total` counts every keyword line.
    Lines {
        lines: Vec<String>,
        /// A shown line was cut to its keyword window.
        clipped: bool,
        first: u32,
        last: u32,
        /// The hit that best fits the goal ([`hit_score`]).
        best: u32,
        total: usize,
        line_count: usize,
        /// The best hit's line, verbatim and trimmed, when it declares a
        /// block (a function, class, ...) and no other line of the file
        /// holds it: a `matchString` + `block` read returns that whole
        /// declaration.
        declaration: Option<String>,
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
    goal: &str,
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
            Ok(content) => scan(&content.bytes, path, keywords, goal, security),
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
    goal: &str,
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
    let mut line_count = 0;
    let lowered: Vec<String> = text.lines().map(str::to_lowercase).collect();
    let terms = goal_terms(goal, &needles);
    // Every keyword line: (index, byte of its first keyword, holds every keyword).
    let mut hits = Vec::new();
    let mut scores = Vec::new();
    for (index, lower) in lowered.iter().enumerate() {
        line_count = index + 1;
        let Some((at, needle)) = needles
            .iter()
            .filter_map(|needle| lower.find(needle.as_str()).map(|at| (at, needle)))
            .min()
        else {
            continue;
        };
        scores.push(hit_score(&lowered, index, at + needle.len(), &terms));
        let every = needles.iter().all(|needle| lower.contains(needle.as_str()));
        hits.push((index, at, every));
    }
    let total = hits.len();
    let raw: Vec<&str> = text.lines().collect();
    // The read anchors where the listing does: on lines holding every
    // keyword when any exist; else on the smallest window holding every
    // keyword (one opened by a declaration first); else on any keyword line.
    let any_every = hits.iter().any(|hit| hit.2);
    let window = (!any_every && needles.len() > 1)
        .then(|| {
            covering_window(&hits, &lowered, &needles, |hit| {
                let (index, at, _) = hits[hit];
                declared_head(raw[index], &lowered, index, at, path, security).is_some()
            })
        })
        .flatten();
    let (mut first, mut last, mut best) = (0, 0, (0, (0, 0)));
    if let Some((start, end)) = window {
        let number = |hit: usize| u32::try_from(hits[hit].0 + 1).unwrap_or(u32::MAX);
        (first, last, best) = (number(start), number(end), (number(start), scores[start]));
    }
    for (&(index, _, every), &score) in hits.iter().zip(&scores) {
        if window.is_some() || (any_every && !every) {
            continue;
        }
        let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
        if first == 0 || score > best.1 {
            best = (number, score);
        }
        if first == 0 {
            first = number;
        }
        last = number;
    }
    // Listed: the lines holding every keyword; without one, each keyword's
    // first line and the anchoring window's first and last lines. The rest stay counted for
    // the whole-file read.
    let mut uncovered: Vec<&String> = if any_every {
        Vec::new()
    } else {
        needles.iter().collect()
    };
    let in_window = |hit: usize| window.is_some_and(|(start, end)| hit == start || hit == end);
    let mut lines = Vec::new();
    let mut clipped = false;
    for (hit, &(index, at, every)) in hits.iter().enumerate() {
        if lines.len() >= MAX_LINES_PER_FILE {
            break;
        }
        let lower = &lowered[index];
        let before = uncovered.len();
        uncovered.retain(|needle| !lower.contains(needle.as_str()));
        if !every && uncovered.len() == before && !in_window(hit) {
            continue;
        }
        let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
        let line = raw[index];
        let in_key_block = key_blocks
            .iter()
            .any(|&(start, end)| (start..=end).contains(&number));
        let shown = if in_key_block {
            crate::security::key_fragment_placeholder()
        } else {
            clipped |= line.chars().count() > MAX_LINE_CHARS;
            let window = clip(line, lower_to_char_index(line, lower, at));
            security
                .sanitize(&window, Path::new(path))
                .map(|(clean, _)| clean)
                .unwrap_or_else(|_| "[REDACTED]".to_owned())
        };
        lines.push(format!(
            "{number}{}{shown}",
            crate::tools::numbered::SEPARATOR
        ));
    }
    if total == 0 {
        return FileHits::Unmatched;
    }
    let declaration = hits
        .iter()
        .find(|hit| u32::try_from(hit.0 + 1).ok() == Some(best.0))
        .and_then(|&(index, at, _)| {
            let number = u32::try_from(index + 1).ok()?;
            if key_blocks
                .iter()
                .any(|&(start, end)| (start..=end).contains(&number))
            {
                return None;
            }
            declared_head(raw[index], &lowered, index, at, path, security)
        });
    FileHits::Lines {
        lines,
        clipped,
        first,
        last,
        best: best.0,
        total,
        line_count,
        declaration,
    }
}

/// Widest span (in lines) of a window that anchors a multi-keyword read.
const MAX_WINDOW_LINES: usize = 60;

/// The hits (`start..=end` indexes into `hits`) of the smallest window of at
/// most [`MAX_WINDOW_LINES`] lines holding every keyword, preferring one
/// opened by a declaration (`declares`), then the narrowest, then the
/// earliest. `None` when no such window exists.
fn covering_window(
    hits: &[(usize, usize, bool)],
    lowered: &[String],
    needles: &[String],
    declares: impl Fn(usize) -> bool,
) -> Option<(usize, usize)> {
    // ((opened by no declaration, span, start), (start, end)).
    type Ranked = ((bool, usize, usize), (usize, usize));
    let mut chosen: Option<Ranked> = None;
    for start in 0..hits.len() {
        let mut missing: Vec<&String> = needles.iter().collect();
        for end in start..hits.len() {
            let span = hits[end].0 - hits[start].0;
            if span > MAX_WINDOW_LINES {
                break;
            }
            missing.retain(|needle| !lowered[hits[end].0].contains(needle.as_str()));
            if missing.is_empty() {
                let rank = (!declares(start), span, start);
                if chosen.as_ref().is_none_or(|(best, _)| rank < *best) {
                    chosen = Some((rank, (start, end)));
                }
                break;
            }
        }
    }
    chosen.map(|(_, window)| window)
}

/// Keywords that open a named block: the word right before a declared name.
const BLOCK_KEYWORDS: &[&str] = &[
    "fn",
    "def",
    "func",
    "function",
    "class",
    "struct",
    "interface",
    "enum",
    "trait",
    "impl",
    "module",
    "object",
    "record",
];

/// The hit line `index` as a `matchString` when its keyword is the name a
/// block keyword declares (`def merge_setting(`, `pub fn spawn`), the line
/// is short and unique in the file, and scanning leaves it unchanged.
fn declared_head(
    line: &str,
    lowered: &[String],
    index: usize,
    at: usize,
    path: &str,
    security: &impl ContentScan,
) -> Option<String> {
    let lower = &lowered[index];
    if hit_shape(lower, at) == 0 {
        return None;
    }
    let before = lower.get(..at)?.trim_end();
    let keyword = before
        .rsplit(|c: char| c.is_whitespace())
        .next()
        .unwrap_or_default();
    // Go methods name their receiver first: `func (r *T) Name(`.
    let go_method = before.trim_start().starts_with("func (") && before.ends_with(')');
    // A keyword that itself opens with the block keyword (`def merge_setting`).
    let leads_with_keyword = lower
        .get(at..)
        .and_then(|hit| hit.split_whitespace().next())
        .is_some_and(|word| BLOCK_KEYWORDS.contains(&word));
    if !BLOCK_KEYWORDS.contains(&keyword) && !go_method && !leads_with_keyword {
        return None;
    }
    let head = line.trim();
    if head.is_empty() || head.chars().count() > MAX_LINE_CHARS {
        return None;
    }
    let needle = head.to_lowercase();
    if lowered
        .iter()
        .filter(|other| other.contains(&needle))
        .count()
        != 1
    {
        return None;
    }
    let (clean, _) = security.sanitize(head, Path::new(path)).ok()?;
    (clean == head).then_some(clean)
}

/// Lines on each side of a hit searched for the goal's other terms.
const GOAL_CONTEXT_LINES: usize = 8;

/// Words of a goal that say what to look for, not how to ask.
const GOAL_STOP_WORDS: &[&str] = &[
    "the",
    "and",
    "for",
    "with",
    "from",
    "that",
    "this",
    "into",
    "find",
    "where",
    "what",
    "which",
    "how",
    "does",
    "show",
    "used",
    "uses",
    "use",
    "code",
    "file",
    "files",
    "line",
    "lines",
    "read",
    "search",
    "look",
    "locate",
    "repo",
    "repository",
    "its",
    "are",
    "was",
    "were",
    "has",
    "have",
    "who",
    "why",
    "when",
];

/// Lowercase goal words (three or more characters) that are not stop words
/// and not part of a searched keyword: the evidence a hit's neighborhood can
/// add beyond the keyword itself.
fn goal_terms(goal: &str, needles: &[String]) -> Vec<String> {
    let mut terms: Vec<String> = goal
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.chars().count() >= 3 && !GOAL_STOP_WORDS.contains(word))
        .filter(|word| !needles.iter().any(|needle| needle.contains(word)))
        .map(str::to_owned)
        .collect();
    terms.sort_unstable();
    terms.dedup();
    terms
}

/// How well hit line `index` (keyword ending at byte `after`) fits the goal,
/// ordered by the hit's own shape first (a literal value assigned to the
/// keyword, `x = 512` or `x: 512`, 3; any other assignment or declaration 2;
/// other code 1; a comment 0), then by the goal terms named within
/// [`GOAL_CONTEXT_LINES`]. Shape leads because neighboring prose names goal
/// terms near uses and definitions alike.
fn hit_score(lowered: &[String], index: usize, after: usize, terms: &[String]) -> (usize, usize) {
    let window = &lowered[index.saturating_sub(GOAL_CONTEXT_LINES)
        ..(index + GOAL_CONTEXT_LINES + 1).min(lowered.len())];
    let named = terms
        .iter()
        .filter(|term| window.iter().any(|line| line.contains(term.as_str())))
        .count();
    (hit_shape(&lowered[index], after), named)
}

fn hit_shape(lower: &str, after: usize) -> usize {
    let code = lower.trim_start();
    if ["//", "#", "/*", "*", "--", "<!--", "\"\"\""]
        .iter()
        .any(|marker| code.starts_with(marker))
    {
        return 0;
    }
    let rest = lower.get(after..).unwrap_or_default().trim_start();
    let value = rest
        .strip_prefix(":=")
        .or_else(|| {
            rest.strip_prefix('=')
                .filter(|value| !value.starts_with(['=', '>']))
        })
        .or_else(|| {
            rest.strip_prefix(':')
                .filter(|value| !value.starts_with(':'))
        })
        .map(str::trim_start);
    if let Some(value) = value {
        let literal = value
            .starts_with(|c: char| c.is_ascii_digit() || matches!(c, '"' | '\'' | '`'))
            || (value.starts_with('-') && value[1..].starts_with(|c: char| c.is_ascii_digit()))
            || ["true", "false", "none", "null", "nil"]
                .iter()
                .any(|word| value.starts_with(word));
        return if literal { 3 } else { 2 };
    }
    let before = lower.get(..after).unwrap_or_default();
    let declared = [
        "fn ",
        "def ",
        "func ",
        "function ",
        "class ",
        "const ",
        "let ",
        "var ",
        "static ",
        "type ",
        "struct ",
        "interface ",
        "enum ",
    ]
    .iter()
    .any(|word| before.contains(word));
    if declared { 2 } else { 1 }
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

    use crate::security::scan::Passthrough;

    /// E10: a hit on a declared name carries its declaration line, so the
    /// read lead returns the whole block; a call or a repeated line does not.
    #[test]
    fn a_declared_hit_names_its_unique_declaration_line() {
        let declaration = |text: &str, keyword: &str| match scan(
            text.as_bytes(),
            "a.py",
            &[keyword.into()],
            "",
            &Passthrough,
        ) {
            FileHits::Lines { declaration, .. } => declaration,
            other => panic!("{other:?}"),
        };
        let text =
            "x = 1\n\ndef merge_setting(request, session):\n    return 1\n\nmerge_setting(a, b)\n";
        assert_eq!(
            declaration(text, "merge_setting"),
            Some("def merge_setting(request, session):".to_owned())
        );
        assert_eq!(declaration("merge_setting(a, b)\n", "merge_setting"), None);
        // The searched keyword may carry the block keyword itself.
        assert_eq!(
            declaration(text, "def merge_setting"),
            Some("def merge_setting(request, session):".to_owned())
        );
        assert_eq!(
            declaration(
                "def merge_setting(a):\n    pass\ndef merge_setting(a):\n    pass\n",
                "merge_setting"
            ),
            None
        );
        assert_eq!(
            declaration(
                "func (s *Server) ListenAndServe() error {\n}\n",
                "listenandserve"
            ),
            Some("func (s *Server) ListenAndServe() error {".to_owned())
        );
        assert_eq!(
            declaration("# def merge_setting(a):\n", "merge_setting"),
            None
        );
    }

    #[test]
    fn every_keyword_line_is_numbered_case_insensitively() {
        let text = "import x\nawait Wrap_App(app)\nother\n    await wrap_app(s)\n";
        let hits = scan(
            text.as_bytes(),
            "a.py",
            &["wrap_app".into()],
            "",
            &Passthrough,
        );
        assert_eq!(
            hits,
            FileHits::Lines {
                lines: vec![
                    "2\tawait Wrap_App(app)".into(),
                    "4\t    await wrap_app(s)".into()
                ],
                first: 2,
                last: 4,
                best: 2,
                total: 2,
                line_count: 4,
                clipped: false,
                declaration: None,
            }
        );
        assert_eq!(
            scan(
                text.as_bytes(),
                "a.py",
                &["absent".into()],
                "",
                &Passthrough
            ),
            FileHits::Unmatched
        );
        assert_eq!(
            scan(
                b"bin\0ary needle",
                "a.bin",
                &["needle".into()],
                "",
                &Passthrough
            ),
            FileHits::Unavailable
        );
    }

    /// The read anchors on the hit that best fits the goal: a literal
    /// assigned to the keyword beats an earlier type declaration, a later
    /// use, and a setter whose docs name more goal terms; goal terms nearby
    /// break ties between hits of the same shape.
    #[test]
    fn the_best_hit_for_the_goal_anchors_the_read() {
        let mut text = String::from(
            "struct Builder {\n    /// Cap on thread usage.\n    max_blocking_threads: usize,\n}\n",
        );
        text.push_str(&"\n".repeat(20));
        text.push_str("fn new() -> Builder {\n    Builder {\n        // Defaults for the pool.\n        max_blocking_threads: 512,\n    }\n}\n");
        text.push_str(&"\n".repeat(20));
        text.push_str("fn spawn(&self) {\n    pool(self.max_blocking_threads);\n}\n");
        // A setter whose docs name more goal terms is still a declaration,
        // not the default value.
        text.push_str("/// The default value is set by the Builder runtime.\npub fn max_blocking_threads(&mut self, val: usize) {\n    self.max_blocking_threads = val;\n}\n");
        let best = |goal: &str| match scan(
            text.as_bytes(),
            "builder.rs",
            &["max_blocking_threads".into()],
            goal,
            &Passthrough,
        ) {
            FileHits::Lines { first, best, .. } => (first, best),
            other => panic!("{other:?}"),
        };
        assert_eq!(
            best("Find default max_blocking_threads in the Builder"),
            (3, 28)
        );
        // Without goal terms a literal assignment still outranks a type.
        assert_eq!(best(""), (3, 28));
        // A comment hit never anchors over code.
        let commented =
            "// max_blocking_threads = 1 by default\nlet max_blocking_threads = config();\n";
        let FileHits::Lines { best, .. } = scan(
            commented.as_bytes(),
            "a.rs",
            &["max_blocking_threads".into()],
            "default max_blocking_threads",
            &Passthrough,
        ) else {
            panic!("hit");
        };
        assert_eq!(best, 2);
    }

    #[test]
    fn long_lines_keep_a_window_around_the_keyword() {
        let line = format!("{}needle{}", "a".repeat(500), "b".repeat(500));
        let FileHits::Lines { lines, clipped, .. } = scan(
            line.as_bytes(),
            "a.js",
            &["needle".into()],
            "",
            &Passthrough,
        ) else {
            panic!("hit");
        };
        // The cut is flagged so the page carries a read of the whole line.
        assert!(clipped);
        assert!(lines[0].starts_with("1\t…"), "{}", lines[0]);
        assert!(lines[0].contains("needle"));
        assert!(lines[0].chars().count() <= MAX_LINE_CHARS + 4);
    }

    /// Several keywords list the lines holding every keyword; lines with
    /// only some stay counted for the readHits continuation. A file where no
    /// line holds them all lists each keyword's first line.
    #[test]
    fn lines_with_every_keyword_are_listed_first() {
        let keywords = ["octocode".to_owned(), "mcp".to_owned()];
        let text =
            "# Octocode\nOctocode MCP server\nmcp only\noctocode only\nthe octocode mcp tools\n";
        let FileHits::Lines { lines, total, .. } =
            scan(text.as_bytes(), "README.md", &keywords, "", &Passthrough)
        else {
            panic!("hit");
        };
        assert_eq!(
            lines,
            vec!["2\tOctocode MCP server", "5\tthe octocode mcp tools"]
        );
        assert_eq!(total, 5);
        let spread = "octocode one\noctocode two\nmcp three\nmcp four\n";
        let FileHits::Lines { lines, total, .. } =
            scan(spread.as_bytes(), "a.md", &keywords, "", &Passthrough)
        else {
            panic!("hit");
        };
        // The window anchoring the read (lines 2-3) is listed too.
        assert_eq!(
            lines,
            vec!["1\toctocode one", "2\toctocode two", "3\tmcp three"]
        );
        assert_eq!(total, 4);
    }

    #[test]
    fn the_read_anchors_on_a_line_holding_every_keyword() {
        let keywords = ["fn".to_owned(), "search_path".to_owned()];
        let text = "fn default() {}\nfn other() {}\nlet x = 1;\nfn search_path(p: &Path) {}\n";
        let FileHits::Lines {
            first, last, best, ..
        } = scan(text.as_bytes(), "a.rs", &keywords, "", &Passthrough)
        else {
            panic!("hit");
        };
        assert_eq!((first, last, best), (4, 4, 4));
    }

    /// GC3: with no line holding every keyword, the read anchors on the
    /// smallest window that holds them all, preferring one a declaration
    /// opens, not on the best single-keyword hit elsewhere in the file.
    #[test]
    fn spread_keywords_anchor_on_the_declaration_window() {
        let mut lines = vec![String::new(); 900];
        lines[128] = "        self.trust_env = trust_env".into();
        lines[491] = "        self.trust_env = True".into();
        lines[640] = "        settings = self.merge_environment_settings(".into();
        lines[830] =
            "    def merge_environment_settings(self, url, proxies, stream, verify, cert):".into();
        lines[844] = "        if self.trust_env:".into();
        let text = lines.join("\n");
        let keywords = [
            "merge_environment_settings".to_owned(),
            "trust_env".to_owned(),
        ];
        let FileHits::Lines {
            lines,
            first,
            last,
            best,
            declaration,
            ..
        } = scan(text.as_bytes(), "sessions.py", &keywords, "", &Passthrough)
        else {
            panic!("hit");
        };
        assert_eq!((first, last, best), (831, 845, 831));
        assert_eq!(
            declaration.as_deref(),
            Some("def merge_environment_settings(self, url, proxies, stream, verify, cert):")
        );
        let numbers: Vec<&str> = lines
            .iter()
            .filter_map(|line| line.split('\t').next())
            .collect();
        for number in ["129", "641", "831", "845"] {
            assert!(numbers.contains(&number), "{lines:?}");
        }
        // Keywords never within the window size keep the best single hit.
        let far =
            "x = trust_env\n".to_owned() + &"\n".repeat(100) + "merge_environment_settings()\n";
        let FileHits::Lines { best, .. } =
            scan(far.as_bytes(), "a.py", &keywords, "", &Passthrough)
        else {
            panic!("hit");
        };
        assert_eq!(best, 1);
    }

    #[test]
    fn many_hits_are_capped_but_counted() {
        let text = "needle\n".repeat(30);
        let FileHits::Lines {
            lines,
            total,
            last,
            clipped,
            ..
        } = scan(
            text.as_bytes(),
            "a.rs",
            &["needle".into()],
            "",
            &Passthrough,
        )
        else {
            panic!("hit");
        };
        assert_eq!(lines.len(), MAX_LINES_PER_FILE);
        assert_eq!(total, 30);
        assert_eq!(last, 30);
        assert!(!clipped);
    }
}
