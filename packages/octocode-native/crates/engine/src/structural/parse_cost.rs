//! What a structural parse of a source costs in memory, estimated from its
//! bytes before it is parsed: a minified one-line file holds as many nodes
//! as a long file of the same size, so its byte length alone says too little.

/// Memory one token of source costs a parse and its matching, measured on a
/// minified one-line file (the densest source: about 580 bytes of memory per
/// source byte, a token a byte).
pub const BYTES_PER_TOKEN: usize = 600;

/// The tokens of `bytes`, counted as runs of word bytes and single other
/// non-space bytes: what a grammar's lexer would cut it into, at most.
#[must_use]
pub(super) fn tokens(bytes: &[u8]) -> usize {
    let (mut tokens, mut in_word) = (0usize, false);
    for &byte in bytes {
        let word = byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80;
        if word {
            tokens += usize::from(!in_word);
            in_word = true;
        } else {
            in_word = false;
            tokens += usize::from(!byte.is_ascii_whitespace());
        }
    }
    tokens
}

/// The memory a parse of `bytes` and the matching over it is estimated to hold.
#[must_use]
pub fn parse_cost(bytes: &[u8]) -> usize {
    tokens(bytes).saturating_mul(BYTES_PER_TOKEN)
}

/// The most memory one parse may be estimated to hold: the documented memory
/// budget ([`crate::memory::MEMORY`]), which also bounds the whole files
/// admitted at once ([`crate::memory::WHOLE_FILES`]). A scan reads it once,
/// on the thread that starts it.
#[must_use]
pub fn parse_budget() -> usize {
    #[cfg(test)]
    if let Some(budget) = test_budget::get() {
        return budget;
    }
    crate::memory::MEMORY.limit()
}

#[cfg(test)]
pub(crate) mod test_budget;

#[cfg(test)]
mod tests;
