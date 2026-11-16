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
pub fn tokens(bytes: &[u8]) -> usize {
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
/// budget ([`crate::memory::WHOLE_FILES`]).
#[must_use]
pub fn parse_budget() -> usize {
    crate::memory::WHOLE_FILES.limit()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minified(calls: usize) -> Vec<u8> {
        format!(
            "function f(x){{return x}}var a=[{}eval(zzz)];\n",
            "f(1),".repeat(calls)
        )
        .into_bytes()
    }

    #[test]
    fn a_dense_minified_file_costs_more_than_a_long_plain_file_of_its_size() {
        let dense = minified(20_000);
        let plain = "let value = compute_something(argument);\n"
            .repeat(dense.len() / 40)
            .into_bytes();
        assert!(parse_cost(&dense) > 2 * parse_cost(&plain));
    }

    #[test]
    fn a_pathological_single_line_is_over_a_small_budget_and_a_normal_file_is_not() {
        let budget = 64 * 1024 * 1024;
        // 9.4 MB of minified calls, as measured at about 2 GB.
        assert!(parse_cost(&minified(1_886_000)) > budget * 8);
        let normal = "fn main() { run(); }\n".repeat(2_000).into_bytes();
        assert!(parse_cost(&normal) < budget);
    }
}
