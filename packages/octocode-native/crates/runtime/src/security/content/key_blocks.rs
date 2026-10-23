use super::{is_private_key_begin, is_private_key_end};

/// Key blocks one source may report separately. A source with more is
/// treated as key material from the block that exceeds the cap to its end,
/// so tracker state stays bounded however large the streamed source is.
pub(crate) const MAX_KEY_BLOCK_RANGES: usize = 4096;

/// What a line inside a private-key block looks like, after stripping the
/// quoting and concatenation a key pasted into source carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LineShape {
    /// Empty, or only quotes and punctuation.
    Blank,
    /// Base64 (`A-Za-z0-9+/=`), optionally quoted and ended with an escaped
    /// newline, `+`, `,` or `;`: `"MIIE…\n" +`.
    Body,
    /// Anything else.
    Other,
}

/// Incremental [`LineShape`] classifier: a line is leading decoration, a
/// base64 core, then trailing decoration. Fed byte pieces so a streamed
/// source never buffers a whole line.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LineShapeScan {
    stage: ShapeStage,
    core: bool,
    escape: bool,
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
            escape: false,
        }
    }
}

impl LineShapeScan {
    pub(crate) fn feed(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if self.stage == ShapeStage::Bad {
                return;
            }
            if self.escape {
                // Only an escaped newline, return or tab is decoration.
                self.escape = false;
                if !matches!(byte, b'n' | b'r' | b't') {
                    self.stage = ShapeStage::Bad;
                }
                continue;
            }
            let base64 = byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=');
            let decoration = byte.is_ascii_whitespace()
                || matches!(
                    byte,
                    b'"' | b'\'' | b'`' | b',' | b';' | b'.' | b'(' | b')' | b'[' | b']' | b'}'
                );
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
                    ShapeStage::Core
                }
                (ShapeStage::Lead, _) if decoration => ShapeStage::Lead,
                (ShapeStage::Core, _) if base64 => ShapeStage::Core,
                (ShapeStage::Core | ShapeStage::Trail, _) if decoration => ShapeStage::Trail,
                (ShapeStage::Trail, b'+') => ShapeStage::Trail,
                _ => ShapeStage::Bad,
            };
        }
    }

    pub(crate) fn finish(self) -> LineShape {
        match self.stage {
            ShapeStage::Bad => LineShape::Other,
            _ if self.core => LineShape::Body,
            _ => LineShape::Blank,
        }
    }
}

/// `Name: value` armor header (`Proc-Type: 4,ENCRYPTED`, `Version: …`),
/// judged from a line's leading bytes.
pub(crate) fn is_key_header_prefix(head: &[u8]) -> bool {
    let head = head.trim_ascii_start();
    let name = head
        .iter()
        .take_while(|byte| byte.is_ascii_alphanumeric() || **byte == b'-')
        .count();
    name > 0 && head[0].is_ascii_alphabetic() && head.get(name) == Some(&b':')
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum BlockPhase {
    #[default]
    Outside,
    /// After the BEGIN marker, before the first body line: armor headers and
    /// blank lines are allowed.
    Header,
    Body,
}

/// Per-line private-key block state shared by the whole-file redaction, the
/// range tracker and the streamed read. A block opens at a BEGIN marker and
/// ends at its END marker (inclusive) or, with no END, before the first line
/// that is not part of a key: armor headers and blank lines straight after
/// BEGIN, then base64 body lines. Everything inside is key material; a
/// literal marker string in ordinary code therefore hides only its own line.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct KeyBlockState {
    phase: BlockPhase,
}

/// How a line reads once it is read inside an open block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BlockLine {
    Header,
    Shape(LineShape),
}

impl KeyBlockState {
    /// Advance over one line; true when it is inside a block (key material).
    /// `inside` is only called while a block is open and the line is not an
    /// END marker.
    pub(crate) fn advance(
        &mut self,
        begins: bool,
        ends: bool,
        inside: impl FnOnce() -> BlockLine,
    ) -> bool {
        if self.phase != BlockPhase::Outside {
            if ends {
                self.phase = BlockPhase::Outside;
                return true;
            }
            match (self.phase, inside()) {
                (_, BlockLine::Shape(LineShape::Body)) => {
                    self.phase = BlockPhase::Body;
                    return true;
                }
                (BlockPhase::Header, BlockLine::Header | BlockLine::Shape(LineShape::Blank)) => {
                    return true;
                }
                _ => self.phase = BlockPhase::Outside,
            }
        }
        if begins {
            if !ends {
                self.phase = BlockPhase::Header;
            }
            return true;
        }
        false
    }

    /// True while a block is open: the next line may still be key material.
    pub(crate) fn is_open(&self) -> bool {
        self.phase != BlockPhase::Outside
    }

    pub(crate) fn push_line(&mut self, line: &str) -> bool {
        self.advance(is_private_key_begin(line), is_private_key_end(line), || {
            let mut scan = LineShapeScan::default();
            scan.feed(line.as_bytes());
            match scan.finish() {
                LineShape::Other if is_key_header_prefix(line.as_bytes()) => BlockLine::Header,
                shape => BlockLine::Shape(shape),
            }
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
    let span = value.lines().count().max(1) as u32;
    let start = line.saturating_sub(span);
    let end = line.saturating_add(span);
    ranges.iter().any(|&(s, e)| start <= e && s <= end)
}
