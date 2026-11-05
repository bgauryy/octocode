//! Reads a file too large to hold whole through the mention tokenizer, one
//! window at a time: a window ends on a byte no token holds, and the next one
//! keeps that byte so a quote around a token stays beside it.
use super::mention_tokens::{Keys, is_token_byte, scan_text};
use std::collections::BTreeMap;
use std::io::Read;

/// Bytes read before a window is scanned; a token longer than this extends
/// its window.
pub(super) const WINDOW: usize = 1 << 20;

/// [`scan_text`] of `source`, stopping when `stopped` says or at binary
/// content. Lines are the file's: a later window continues the count.
pub(super) fn scan_stream<'k>(
    mut source: impl Read,
    code: bool,
    keys: &Keys<'k>,
    stopped: &(dyn Fn() -> bool + Sync),
) -> [BTreeMap<&'k str, usize>; 3] {
    let mut found: [BTreeMap<&'k str, usize>; 3] = Default::default();
    let mut held = Vec::with_capacity(2 * WINDOW);
    let mut lines_before = 0;
    // The kept byte that starts `held`, which no token holds.
    let mut kept = 0;
    let mut chunk = vec![0; WINDOW];
    while !stopped() {
        let read = match source.read(&mut chunk) {
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => 0,
        };
        held.extend_from_slice(&chunk[..read]);
        let end = if read == 0 {
            held.len()
        } else {
            match held[kept..].iter().rposition(|byte| !is_token_byte(*byte)) {
                Some(last) => kept + last + 1,
                None => continue,
            }
        };
        let Ok(decoded) =
            octocode_engine::text::admit(&held[..end], octocode_engine::text::Decoding::Lenient)
        else {
            break;
        };
        for (hits, local) in found.iter_mut().zip(scan_text(&decoded.text, code, keys)) {
            for (key, line) in local {
                hits.entry(key).or_insert(line + lines_before);
            }
        }
        lines_before += memchr::memchr_iter(b'\n', &held[..end]).count();
        if read == 0 {
            break;
        }
        kept = usize::from(held[end - 1] != b'\n');
        held.drain(..end - kept);
    }
    found
}
