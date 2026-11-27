//! A line too long for a window, judged alone in overlapping windows: the text
//! a match scan of it runs over keeps every byte no redaction covers, once,
//! and shows no secret byte, whatever the scanner writes and wherever its
//! windows' redactions overlap. Every value is fake.
use super::judged_alone;
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_runtime_base::security::SourceLimits;
use octocode_runtime_base::security::scan::ContentScan;
use std::path::Path;

mod streamed;

const TOKEN: &str = "ghp_aB3dE5gH7jK9mN1pQ3sT5vX7zA9cE1gI3kM5";
const PLACEHOLDER: &str = "[REDACTED-GITHUBTOKENS]";
const LIMITS: SourceLimits = SourceLimits::SMALL;

/// The windows `judged_alone` judges a line in: [`SourceLimits::window`]
/// bytes, each overlapping the next by a quarter.
fn windows(len: usize) -> Vec<(usize, usize)> {
    let (size, overlap) = (LIMITS.window, LIMITS.window / 4);
    let mut windows = vec![(0, size.min(len))];
    while let Some(&(_, end)) = windows.last().filter(|window| window.1 < len) {
        windows.push((end - overlap, (end - overlap + size).min(len)));
    }
    windows
}

/// `len` bytes of one line whose every 9-byte record is its own position, with
/// each of `marks` written over the filler at its byte.
fn line(len: usize, marks: &[(usize, &str)]) -> String {
    let mut line = (0..len.div_ceil(9))
        .map(|at| format!("{at:08};"))
        .collect::<String>()
        .into_bytes();
    line.truncate(len);
    for &(at, mark) in marks {
        line[at..at + mark.len()].copy_from_slice(mark.as_bytes());
    }
    String::from_utf8(line).expect("ascii")
}

/// Hits every 9,001 bytes, `secret` every 4,099 bytes, and a `secret` across
/// each window's start and end, so the windows' redactions meet in every
/// overlap.
fn spread(len: usize, secret: &str) -> (String, usize) {
    let mut marks: Vec<(usize, &str)> = Vec::new();
    for (start, end) in windows(len) {
        for edge in [start, end] {
            if edge > 20 && edge + secret.len() < len {
                marks.push((edge - 10, secret));
            }
        }
    }
    marks.extend((1..len / 4_099).map(|step| (step * 4_099, secret)));
    marks.sort_unstable();
    marks.dedup_by(|later, kept| later.0 < kept.0 + kept.1.len() + 8);
    let free = |at: usize| {
        !marks
            .iter()
            .any(|&(mark, text)| at + 6 + 8 > mark && at < mark + text.len() + 8)
    };
    let hits = (1..len / 9_001)
        .map(|step| step * 9_001)
        .filter(|&at| free(at))
        .collect::<Vec<_>>();
    let all = marks
        .iter()
        .copied()
        .chain(hits.iter().map(|&at| (at, "NEEDLE")))
        .collect::<Vec<_>>();
    (line(len, &all), hits.len())
}

/// A scanner whose placeholder no placeholder pattern knows: each `SECRET`
/// becomes `<hidden>`.
struct Unknown;
impl ContentScan for Unknown {
    fn sanitize(&self, text: &str, _: &Path) -> Result<(String, Vec<String>), (ErrorCode, String)> {
        Ok((text.replace("SECRET", "<hidden>"), Vec::new()))
    }
    fn limits(&self) -> SourceLimits {
        LIMITS
    }
}

/// The real scanner, which refuses every window holding `REFUSE`.
struct Refusing(octocode_runtime_base::security::ContentSecurity);
impl ContentScan for Refusing {
    fn sanitize(
        &self,
        text: &str,
        path: &Path,
    ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
        if text.contains("REFUSE") {
            return Err((ErrorCode::ContentSecurityLimit, "refused".to_owned()));
        }
        self.0.sanitize(text, path)
    }
    fn limits(&self) -> SourceLimits {
        LIMITS
    }
}

fn len() -> usize {
    LIMITS.whole_source + LIMITS.whole_source / 4
}

/// The real detector: the line is its text with each token's placeholder, every
/// other byte once, whatever window saw the token whole.
#[test]
fn the_real_detector_shows_every_other_byte_once() {
    let token = format!(" token={TOKEN} ");
    let (raw, hits) = spread(len(), &token);
    let judged = judged_alone(
        &raw,
        Path::new("dump.txt"),
        &octocode_runtime_base::security::ContentSecurity::with_limits(LIMITS),
        &NeverCancel,
    )
    .expect("judged");
    assert!(!judged.contains(&TOKEN[8..16]), "no token shows");
    assert_eq!(judged.matches("NEEDLE").count(), hits);
    assert!(
        judged == raw.replace(TOKEN, PLACEHOLDER),
        "every other byte once"
    );
}

/// A placeholder no pattern knows: the windows' redactions in each overlap are
/// placed one by one, so the line is its text with each secret's placeholder,
/// and the hits past every overlap are there.
#[test]
fn a_placeholder_no_pattern_knows_keeps_every_other_byte_once() {
    let (raw, hits) = spread(len(), "SECRET");
    let judged = judged_alone(&raw, Path::new("dump.txt"), &Unknown, &NeverCancel).expect("judged");
    assert!(!judged.contains("SECRET"), "no secret shows");
    assert_eq!(judged.matches("NEEDLE").count(), hits, "every hit is there");
    assert!(
        judged == raw.replace("SECRET", "<hidden>"),
        "every other byte once: {} of {} bytes",
        judged.len(),
        raw.replace("SECRET", "<hidden>").len()
    );
}

/// A refused window stays hidden whole, merged with the redactions its
/// neighbours made across its edges: the bytes before and after them all show
/// once, the window shows a placeholder, and no secret byte shows.
#[test]
fn a_refused_window_merges_with_its_neighbours_redactions() {
    let len = len();
    let (start, end) = windows(len)[1];
    let token = format!(" token={TOKEN} ");
    // Tokens across the refused window's edges, each whole in a neighbour.
    let (before, after) = (start - 20, end - 20);
    let refuse = (start + end) / 2;
    let hit = end + 4_000;
    let raw = line(
        len,
        &[
            (before, &token),
            (refuse, "REFUSE"),
            (after, &token),
            (hit, "NEEDLE"),
            (before - 3_000, "NEEDLE"),
        ],
    );
    let scanner = Refusing(octocode_runtime_base::security::ContentSecurity::with_limits(LIMITS));
    let judged = judged_alone(&raw, Path::new("dump.txt"), &scanner, &NeverCancel).expect("judged");
    assert!(!judged.contains(&TOKEN[8..16]), "no token shows");
    assert!(
        !judged.contains("REFUSE"),
        "the refused window stays hidden"
    );
    let head = &raw[..before + " token=".len()];
    let tail = &raw[after + token.len() - 1..];
    assert!(judged.starts_with(head), "the bytes before show once");
    assert!(judged.ends_with(tail), "the bytes after show once");
    let middle = &judged[head.len()..judged.len() - tail.len()];
    assert!(
        middle.contains(octocode_runtime_base::security::cut::REDACTED),
        "the refused window shows its placeholder: {middle}"
    );
    assert_eq!(
        judged.matches("NEEDLE").count(),
        2,
        "the hits around it are there"
    );
}
