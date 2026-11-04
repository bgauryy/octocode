//! The mention scan's tokenizer: runs of name and path characters found by a
//! byte walk, looked up in one key set.
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// The key sets as one lookup: a token's parts are the inner loop, and most
/// are no key. Length by first byte rejects those without hashing.
pub(super) struct Keys<'k> {
    kinds: HashMap<&'k str, u8>,
    /// Per first byte, a bit per key length (the last bit: 63 or longer).
    lengths: Box<[u64; 256]>,
}

const STEM: u8 = 1;
const NAME: u8 = 2;

impl<'k> Keys<'k> {
    pub(super) fn new(stems: &'k BTreeSet<String>, names: &'k BTreeSet<String>) -> Self {
        let mut keys = Self {
            kinds: HashMap::with_capacity(stems.len() + names.len()),
            lengths: Box::new([0; 256]),
        };
        for (set, kind) in [(stems, STEM), (names, NAME)] {
            for key in set {
                *keys.kinds.entry(key).or_default() |= kind;
                if let Some(first) = key.as_bytes().first() {
                    keys.lengths[usize::from(*first)] |= 1 << key.len().min(63);
                }
            }
        }
        keys
    }

    fn of(&self, text: &str, kind: u8) -> Option<&'k str> {
        // The empty key: a dot file's stem is "".
        if let Some(first) = text.as_bytes().first()
            && self.lengths[usize::from(*first)] & (1 << text.len().min(63)) == 0
        {
            return None;
        }
        let (&key, kinds) = self.kinds.get_key_value(text)?;
        (kinds & kind != 0).then_some(key)
    }
}

/// ASCII bytes of a name or path token; other bytes decode to decide.
const TOKEN_BYTES: [bool; 128] = {
    let mut table = [false; 128];
    let mut byte = 0u8;
    while byte < 128 {
        table[byte as usize] =
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$' | b'.' | b'/' | b'@' | b'-');
        byte += 1;
    }
    table
};

/// One file's hits: per kind (stems, references, words), per key, its first
/// line in the file. A token is a run of name and path characters; the scan
/// walks bytes, since all but a little text is ASCII.
pub(super) fn scan_text<'k>(
    text: &str,
    code: bool,
    keys: &Keys<'k>,
) -> [BTreeMap<&'k str, usize>; 3] {
    let bytes = text.as_bytes();
    let mut local: [BTreeMap<&'k str, usize>; 3] = Default::default();
    let mut line = 1;
    let mut at = 0;
    while at < bytes.len() {
        let mut end = at;
        while let Some(&byte) = bytes.get(end) {
            let width = if byte < 0x80 {
                usize::from(TOKEN_BYTES[usize::from(byte)])
            } else {
                text[end..]
                    .chars()
                    .next()
                    .filter(|c| c.is_alphanumeric())
                    .map_or(0, char::len_utf8)
            };
            if width == 0 {
                break;
            }
            end += width;
        }
        if end > at {
            scan_token(text, at, end, code, keys, line, &mut local);
            at = end;
        }
        match bytes.get(at) {
            Some(b'\n') => {
                line += 1;
                at += 1;
            }
            Some(&byte) if byte < 0x80 => at += 1,
            Some(_) => at += text[at..].chars().next().map_or(1, char::len_utf8),
            None => {}
        }
    }
    local
}

/// The hits of the token `text[start..end]` on `line`.
fn scan_token<'k>(
    text: &str,
    start: usize,
    end: usize,
    code: bool,
    keys: &Keys<'k>,
    line: usize,
    local: &mut [BTreeMap<&'k str, usize>; 3],
) {
    let token = &text[start..end];
    if memchr::memchr2(b'/', b'.', token.as_bytes()).is_some() {
        let last = token.rsplit('/').next().unwrap_or(token);
        let stem = last.split('.').next().unwrap_or(last);
        if let Some(stem) = keys.of(stem, STEM) {
            local[0].entry(stem).or_insert(line);
        }
    } else if let Some(stem) = keys.of(token, STEM)
        && quoted(text.as_bytes(), start, end)
    {
        // A bare name as a whole string literal (`'worker'`) is a file a
        // loader may build a path from.
        local[0].entry(stem).or_insert(line);
    }
    let mut from = 0;
    for part in token.split(['.', '/', '-', '@']) {
        let part_start = from;
        from += part.len() + 1;
        let Some(name) = keys.of(part, NAME) else {
            continue;
        };
        let qualified = token[..part_start]
            .strip_suffix(['.', '/'])
            .is_some_and(|head| head.ends_with(|c: char| c.is_alphanumeric() || "_$".contains(c)));
        let kind = if code || qualified { 1 } else { 2 };
        local[kind].entry(name).or_insert(line);
    }
}

/// Whether the token at `start..end` is a whole string literal: the same
/// quote right before and right after it. A quote is one byte, and no byte of
/// a longer character equals one.
fn quoted(bytes: &[u8], start: usize, end: usize) -> bool {
    let before = start.checked_sub(1).map(|at| bytes[at]);
    matches!(before, Some(b'\'' | b'"' | b'`')) && before == bytes.get(end).copied()
}
