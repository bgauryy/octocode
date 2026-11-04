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
        }
    }
}

impl LineShapeScan {
    pub(crate) fn feed(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            let base64 = byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=');
            self.run = if base64 { self.run + 1 } else { 0 };
            self.run_has_text = base64 && (self.run_has_text || byte.is_ascii_alphanumeric());
            self.long_run |= self.run >= BODY_RUN && self.run_has_text;
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

    /// The fed line as a block reads it; `head` is its leading bytes.
    pub(crate) fn finish(self, head: &[u8]) -> BlockLine {
        BlockLine {
            shape: self.shape(head),
            long: self.long_run,
        }
    }
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
