use super::key_markers::{Markers, after_line_lead, is_decoration, is_lead_mark, line_markers};

/// Key blocks one source may report separately. A source with more is
/// treated as key material from the block that exceeds the cap to its end,
/// so tracker state stays bounded however large the streamed source is.
pub(crate) const MAX_KEY_BLOCK_RANGES: usize = 4096;

/// A base64 run this long inside a block is key material whatever else its
/// line holds (`k += b"MIIE…"`). Every format wraps its body longer than this.
/// Only `/` or `=` is a ruled line, not a run.
const BODY_RUN: usize = 40;

/// What a line inside a private-key block looks like, after stripping the
/// quoting and concatenation a key pasted into source carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LineShape {
    /// Empty, or only quotes and punctuation.
    Blank,
    /// Base64 (`A-Za-z0-9+/=`), optionally quoted, commented, and ended with
    /// an escaped newline, `+`, `,` or `;`: `"MIIE…\n" +`.
    Body,
    /// `Name: value` armor header (`Proc-Type: 4,ENCRYPTED`, `Version: …`).
    Header,
    /// Anything else.
    Other,
}

/// One line as a block reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BlockLine {
    shape: LineShape,
    /// It holds a [`BODY_RUN`]: this is a key's text, not a line near a mention.
    long: bool,
}

/// Incremental [`LineShape`] classifier: a line is leading decoration, a
/// base64 core, then trailing decoration. Fed byte pieces so a streamed
/// source never buffers a whole line.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LineShapeScan {
    stage: ShapeStage,
    core: bool,
    /// Bytes of the core so far, and whether all of them are `/`: `// ` is a
    /// comment, not the start of a body.
    core_len: usize,
    core_slashes: bool,
    /// The core is one `+` so far: before a blank it is a leading concatenation.
    core_plus: bool,
    /// A quote opened before the core: `b"` is a string prefix only before it.
    quoted: bool,
    escape: bool,
    run: usize,
    run_has_text: bool,
    long_run: bool,
    /// Bytes fed so far, and where the line's first [`BODY_RUN`] starts.
    fed: usize,
    long_run_at: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShapeStage {
    Lead,
    Core,
    Trail,
    Bad,
}

impl Default for LineShapeScan {
    fn default() -> Self {
        Self {
            stage: ShapeStage::Lead,
            core: false,
            core_len: 0,
            core_slashes: true,
            core_plus: false,
            quoted: false,
            escape: false,
            run: 0,
            run_has_text: false,
            long_run: false,
            fed: 0,
            long_run_at: None,
        }
    }
}

impl LineShapeScan {
    pub(crate) fn feed(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            let base64 = byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=');
            self.run = if base64 { self.run + 1 } else { 0 };
            self.run_has_text = base64 && (self.run_has_text || byte.is_ascii_alphanumeric());
            self.fed += 1;
            if self.run >= BODY_RUN && self.run_has_text && !self.long_run {
                self.long_run = true;
                self.long_run_at = Some(self.fed - self.run);
            }
            if self.stage == ShapeStage::Bad {
                if self.long_run {
                    return;
                }
                continue;
            }
            if self.escape {
                // Only an escaped newline, return or tab is decoration.
                self.escape = false;
                if !matches!(byte, b'n' | b'r' | b't') {
                    self.stage = ShapeStage::Bad;
                }
                continue;
            }
            let quote = matches!(byte, b'"' | b'\'' | b'`');
            self.stage = match (self.stage, byte) {
                (stage, b'\\') => {
                    self.escape = true;
                    if stage == ShapeStage::Core {
                        ShapeStage::Trail
                    } else {
                        stage
                    }
                }
                (ShapeStage::Lead, _) if base64 => {
                    self.core = true;
                    self.core_len = 1;
                    self.core_slashes = byte == b'/';
                    self.core_plus = byte == b'+';
                    ShapeStage::Core
                }
                (ShapeStage::Lead, _) if is_decoration(byte) || is_lead_mark(byte) => {
                    self.quoted |= quote;
                    ShapeStage::Lead
                }
                (ShapeStage::Core, _) if base64 => {
                    self.core_len += 1;
                    self.core_slashes &= byte == b'/';
                    self.core_plus = false;
                    ShapeStage::Core
                }
                (ShapeStage::Core, _)
                    if (self.core_slashes && (byte.is_ascii_whitespace() || byte == b'*'))
                        || (self.core_plus && byte.is_ascii_whitespace()) =>
                {
                    self.core = false;
                    ShapeStage::Lead
                }
                (ShapeStage::Core, _) if quote && !self.quoted && self.core_len <= 2 => {
                    self.core = false;
                    self.quoted = true;
                    ShapeStage::Lead
                }
                (ShapeStage::Core | ShapeStage::Trail, _) if is_decoration(byte) => {
                    ShapeStage::Trail
                }
                (ShapeStage::Trail, b'+') => ShapeStage::Trail,
                _ => ShapeStage::Bad,
            };
        }
    }

    fn shape(self, head: &[u8]) -> LineShape {
        match self.stage {
            ShapeStage::Bad if self.long_run => LineShape::Body,
            ShapeStage::Bad if is_key_header_prefix(head) => LineShape::Header,
            ShapeStage::Bad => LineShape::Other,
            _ if self.core => LineShape::Body,
            _ => LineShape::Blank,
        }
    }

    /// Where the key starts on the fed line when the line is no key's own
    /// line but holds a key's run ([`LineShape::Body`] by its
    /// [`BODY_RUN`] alone: `logo: iVBOR…`, `k += b"MIIE…"`): what precedes
    /// the run is a label or code, no key text. `None` for a line that is a
    /// body line as a whole, whose key starts after its indentation.
    fn labelled_run_at(&self) -> Option<usize> {
        self.long_run_at.filter(|_| self.stage == ShapeStage::Bad)
    }

    /// The fed line as a block reads it; `head` is its leading bytes.
    pub(crate) fn finish(self, head: &[u8]) -> BlockLine {
        BlockLine {
            shape: self.shape(head),
            long: self.long_run,
        }
    }
}

/// Whether `line` opens with a run of base64 only a key's body is that long:
/// a body line that something cut off, as a hover does (`MIIE…`).
pub(super) fn opens_with_body_run(line: &str) -> bool {
    let text = after_line_lead(line.as_bytes());
    let run = text
        .iter()
        .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(**byte, b'+' | b'/' | b'='))
        .count();
    run >= BODY_RUN
}

/// Whether `line` holds a run of base64 only a key's body is that long,
/// wherever it stands on the line ([`BODY_RUN`]).
pub(super) fn holds_body_run(line: &str) -> bool {
    let mut scan = LineShapeScan::default();
    scan.feed(line.as_bytes());
    scan.long_run
}

/// `Name: value` armor header, judged from a line's leading bytes.
fn is_key_header_prefix(head: &[u8]) -> bool {
    let head = after_line_lead(head);
    let name = head
        .iter()
        .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(**byte, b'-' | b'_'))
        .count();
    name > 0 && head[0].is_ascii_alphabetic() && head.get(name) == Some(&b':')
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum BlockPhase {
    #[default]
    Outside,
    /// After the BEGIN marker: a key-shaped line of any length is key material,
    /// so a body's first line is never shown.
    Opened,
    /// A line that is no key text followed BEGIN, as code after a literal
    /// marker does, or a comment above a body. Only a long body line counts.
    Unproven,
    /// A long body line has shown this is a key: until its END, every
    /// key-shaped line is hidden and any other line is read past.
    Keyed,
}

/// Per-line private-key block state shared by the whole-file redaction, the
/// range tracker and the streamed read. A BEGIN marker opens a block and its
/// END marker (inclusive) closes it. Inside one only key text is hidden: armor
/// headers, blank lines and prose hold no key bytes, and a key may go on after
/// them. A literal marker string in ordinary code therefore hides its own line
/// and at most the single-word lines right after it.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct KeyBlockState {
    phase: BlockPhase,
}

impl KeyBlockState {
    /// Advance over one line with these `markers`; true when it is key
    /// material. `inside` is only called for a line of an open block that holds
    /// no edge of it.
    pub(crate) fn advance(&mut self, markers: Markers, inside: impl FnOnce() -> BlockLine) -> bool {
        if markers.begins || (markers.ends && self.is_open()) {
            // An END that closes no key text is a literal, as outside a block.
            let hidden = markers.begins || self.phase != BlockPhase::Unproven;
            self.phase = if markers.opens {
                BlockPhase::Opened
            } else {
                BlockPhase::Outside
            };
            return hidden;
        }
        if !self.is_open() {
            return false;
        }
        let line = inside();
        match (self.phase, line.shape) {
            (_, LineShape::Body) if line.long => {
                self.phase = BlockPhase::Keyed;
                true
            }
            (BlockPhase::Opened | BlockPhase::Keyed, LineShape::Body) => true,
            (BlockPhase::Opened, LineShape::Other) => {
                self.phase = BlockPhase::Unproven;
                false
            }
            _ => false,
        }
    }

    /// True while a block is open: the next line may still be key material.
    pub(crate) fn is_open(&self) -> bool {
        self.phase != BlockPhase::Outside
    }

    pub(crate) fn push_line(&mut self, line: &str) -> bool {
        let line = line.as_bytes();
        let markers = line_markers(line);
        self.advance(markers, || {
            let mut scan = LineShapeScan::default();
            scan.feed(line);
            scan.finish(line)
        })
    }
}

/// What a line of a key block shows with only its key's bytes
/// ([`key_line_bytes`]) hidden: an armor line that holds no key text reads
/// as written.
pub(crate) fn hide_key_line(line: &str) -> String {
    let Some(key) = key_line_bytes(line) else {
        return line.to_owned();
    };
    format!(
        "{}{}{}",
        &line[..key.start],
        super::key_fragment_placeholder(),
        &line[key.end..]
    )
}

/// The leading bytes of a line that say where its key starts
/// ([`key_line_lead`]): what a streamed read keeps of a line of any length,
/// so every reader of a source finds the same start.
pub(crate) const KEY_LINE_HEAD_BYTES: usize = 256;

/// The bytes of a line of a key block that are its key; `None` when the line
/// holds none. A body line keeps its indentation, and a line that only holds
/// a key's run keeps what precedes the run. An armor line (`-----BEGIN …
/// PRIVATE KEY-----`, `---- END SSH2 … ----`, PuTTY's named lines) names the
/// block and holds none of it: only key text written on the same line is
/// (a block on one line, a body before its END), from its first byte to its
/// last, any armor between them included. What stands beside the armor that
/// is no key's text (a quote, code after a literal marker, a minified
/// document's other members) reads as written, and so does the name of
/// PuTTY's MAC, not its value.
pub(crate) fn key_line_bytes(line: &str) -> Option<std::ops::Range<usize>> {
    let bytes = line.as_bytes();
    let named = after_line_lead(bytes);
    if named.starts_with(b"PuTTY-User-Key-File-") || named.starts_with(b"Private-Lines:") {
        return None;
    }
    if !line_markers(bytes).any() {
        let mut shape = LineShapeScan::default();
        shape.feed(bytes);
        let head = bytes.get(..KEY_LINE_HEAD_BYTES).unwrap_or(bytes);
        let lead = key_line_lead(head, &shape).len();
        return (lead < line.len()).then_some(lead..line.len());
    }
    let named_at = bytes.len() - named.len();
    if let Some(mac) = named.strip_prefix(b"Private-MAC:".as_slice()) {
        let blanks = mac
            .iter()
            .take_while(|byte| matches!(byte, b' ' | b'\t'))
            .count();
        let start = named_at + "Private-MAC:".len() + blanks;
        let end = line.trim_end().len();
        return (start < end).then_some(start..end);
    }
    let edges = armor_edges(bytes);
    let mut pieces = Vec::new();
    if let Some((first, false)) = edges.first() {
        let lead = bytes.len() - after_line_lead(bytes).len();
        pieces.push(lead.min(first.start)..first.start);
    }
    for (index, (edge, begins)) in edges.iter().enumerate() {
        if *begins {
            let to = edges[index + 1..]
                .iter()
                .find(|(_, begins)| !begins)
                .map_or(bytes.len(), |(end, _)| end.start);
            pieces.push(edge.end..to.max(edge.end));
        }
    }
    let keyed: Vec<_> = pieces
        .into_iter()
        .filter(|piece| holds_key_text(&bytes[piece.clone()]))
        .collect();
    Some(keyed.first()?.start..keyed.last()?.end)
}

/// Whether `piece`, text beside a line's armor, is key text: base64 alone in
/// a key line's quoting ([`LineShape::Body`]), or a run only a key's body is
/// that long. A concatenation's `+` or an escape's letter (`\n" +`) is
/// quoting, no base64: key text holds a run of a few base64 characters, a
/// letter among them. A number alone is no key's text: the label a numbered
/// view writes before the armor (`124169\t-----END …`) reads as written.
fn holds_key_text(piece: &[u8]) -> bool {
    const KEY_TEXT_RUN: usize = 4;
    let mut scan = LineShapeScan::default();
    scan.feed(piece);
    let keyed = piece
        .split(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=')))
        .any(|run| run.len() >= KEY_TEXT_RUN && run.iter().any(u8::is_ascii_alphabetic));
    scan.finish(piece).shape == LineShape::Body && keyed
}

/// The armor edges of `line`, in order, each with whether it is a BEGIN:
/// from an opening run of at least four dashes, blanks, `BEGIN` or `END` and
/// the label, through the closing run of dashes (to the line's end when a
/// cut line lost it).
fn armor_edges(line: &[u8]) -> Vec<(std::ops::Range<usize>, bool)> {
    let dashes = |from: usize| {
        line.get(from..)
            .unwrap_or_default()
            .windows(4)
            .position(|window| window == b"----")
            .map(|at| from + at)
    };
    let run_end = |from: usize| {
        from + line[from..]
            .iter()
            .take_while(|byte| **byte == b'-')
            .count()
    };
    let mut edges = Vec::new();
    let mut at = 0;
    while let Some(start) = dashes(at) {
        let opened = run_end(start);
        let word = opened
            + line[opened..]
                .iter()
                .take_while(|byte| matches!(byte, b' ' | b'\t'))
                .count();
        let rest = &line[word..];
        let begins = rest.starts_with(b"BEGIN");
        if !begins && !rest.starts_with(b"END") {
            at = opened;
            continue;
        }
        let label = word + if begins { "BEGIN".len() } else { "END".len() };
        let close = dashes(label).map_or(line.len(), run_end);
        edges.push((start..close, begins));
        at = close;
    }
    edges
}

/// The bytes of a line of a key block before its key: all that precede a
/// BEGIN, what precedes the run of a line that only holds a key's run
/// (`shape`, the whole line's: [`LineShapeScan::labelled_run_at`]), or an END
/// or a body line's indentation. `head` is the line's first
/// [`KEY_LINE_HEAD_BYTES`]: a run that starts past them leaves the line a
/// body line.
pub(crate) fn key_line_lead<'a>(head: &'a [u8], shape: &LineShapeScan) -> &'a [u8] {
    if let Some(at) = head.windows(10).position(|bytes| bytes == b"-----BEGIN") {
        return &head[..at];
    }
    if let Some(label) = shape.labelled_run_at().and_then(|at| head.get(..at)) {
        return label;
    }
    let blanks = head
        .iter()
        .take_while(|byte| matches!(byte, b' ' | b'\t'))
        .count();
    &head[..blanks]
}

/// [`private_key_block_line_ranges`] fed one line at a time, so a caller can
/// stream a source without retaining it. Lines are pushed in order without
/// terminators; [`KeyBlockTracker::finish`] closes an open block at its last
/// key line. Past [`MAX_KEY_BLOCK_RANGES`] blocks the last range is
/// open-ended (`u32::MAX`), redacting every later line.
#[derive(Default)]
pub(crate) struct KeyBlockTracker {
    ranges: Vec<(u32, u32)>,
    block: KeyBlockState,
    start: Option<u32>,
    last_key: u32,
    last: u32,
    saturated: bool,
}

impl KeyBlockTracker {
    pub(crate) fn push(&mut self, line: &str) {
        self.last = self.last.saturating_add(1);
        if self.saturated {
            return;
        }
        let n = self.last;
        if self.block.push_line(line) {
            if self.start.is_none() {
                if self.ranges.len() >= MAX_KEY_BLOCK_RANGES {
                    self.ranges.push((n, u32::MAX));
                    self.saturated = true;
                    return;
                }
                self.start = Some(n);
            }
            self.last_key = n;
            if !self.block.is_open()
                && let Some(start) = self.start.take()
            {
                self.ranges.push((start, n));
            }
        } else if let Some(start) = self.start.take() {
            self.ranges.push((start, self.last_key));
        }
    }

    pub(crate) fn finish(mut self) -> Vec<(u32, u32)> {
        if let Some(start) = self.start.take() {
            self.ranges.push((start, self.last_key));
        }
        self.ranges
    }
}

/// True when a search match's rendered window (its primary `line` plus the
/// lines its `value` spans, extended both ways to cover context) intersects any
/// private-key block range. Conservative on purpose: it may redact a match
/// adjacent to a block, but only within a file that actually contains a private
/// key.
pub(crate) fn match_window_intersects_key_block(
    line: u32,
    value: &str,
    ranges: &[(u32, u32)],
) -> bool {
    let span = octocode_engine::num::saturating_u32(value.lines().count().max(1));
    let start = line.saturating_sub(span);
    let end = line.saturating_add(span);
    ranges.iter().any(|&(s, e)| start <= e && s <= end)
}
