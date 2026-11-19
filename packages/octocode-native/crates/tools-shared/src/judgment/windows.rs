//! What each model's context windows have taken and refused, learned from
//! the provider's own answers. The provider decides what fits; no size is
//! assumed here. A request at least as large as one this endpoint refused for
//! the same model and window, and larger than every one it took, is known to
//! be refused again: its caller can cut the source instead of uploading it
//! whole once more. Sizes between the two bounds are still asked.
//!
//! The bounds are request body bytes of text requests. Tokens per byte vary
//! with the text, so a bound can be wrong for a much sparser one: a larger
//! request the provider takes anywhere in this process lifts it. Kept in
//! process memory: a server learns once, a one-shot command asks each time.
//!
//! A taken request's reported input tokens also teach how many tokens a
//! byte costs here; with the provider's stated token window that sizes the
//! request a source split into sections can send ([`request_bytes`]).
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, PoisonError};

#[derive(Clone, Copy, Default)]
struct Bounds {
    /// The largest request taken.
    taken: usize,
    /// The smallest request refused as too large.
    refused: Option<usize>,
    /// The most input tokens per request byte a taken request reported.
    tokens_per_byte: Option<f64>,
}

/// An endpoint, a model and a window (`true`: the long-context one).
type Window = (String, String, bool);

static SEEN: LazyLock<Mutex<HashMap<Window, Bounds>>> = LazyLock::new(Mutex::default);

fn with<T>(endpoint: &str, model: &str, long: bool, read: impl FnOnce(&mut Bounds) -> T) -> T {
    let mut seen = SEEN.lock().unwrap_or_else(PoisonError::into_inner);
    read(
        seen.entry((endpoint.to_owned(), model.to_owned(), long))
            .or_default(),
    )
}

/// The window took a request of `bytes`, reported as `input_tokens`.
pub(crate) fn took(
    endpoint: &str,
    model: &str,
    long: bool,
    bytes: usize,
    input_tokens: Option<u64>,
) {
    with(endpoint, model, long, |bounds| {
        bounds.taken = bounds.taken.max(bytes);
        if let Some(tokens) = input_tokens.filter(|tokens| *tokens > 0 && bytes > 0) {
            #[expect(
                clippy::cast_precision_loss,
                reason = "a ratio of request sizes, far below f64's exact range"
            )]
            let density = tokens as f64 / bytes as f64;
            bounds.tokens_per_byte = Some(
                bounds
                    .tokens_per_byte
                    .map_or(density, |known| known.max(density)),
            );
        }
    });
}

/// The request size a window of `stated_tokens` can be sent: how many bytes
/// it holds at the densest text seen here (a token covers at least one byte
/// of the text sent, so with none seen, one token per byte), never past the
/// smallest refusal, and at least the largest take below it. `learned`:
/// whether a reported token count, not the one-byte bound, sized it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestFit {
    pub bytes: usize,
    pub learned: bool,
}

pub(crate) fn request_bytes(
    endpoint: &str,
    model: &str,
    long: bool,
    stated_tokens: u64,
) -> RequestFit {
    with(endpoint, model, long, |bounds| {
        let stated = usize::try_from(stated_tokens).unwrap_or(usize::MAX);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "a positive byte count well inside usize, rounded down"
        )]
        let by_tokens = bounds.tokens_per_byte.map_or(stated, |density| {
            (stated_tokens as f64 / density).floor() as usize
        });
        let mut bytes = by_tokens;
        if let Some(refused) = bounds.refused.filter(|refused| *refused > bounds.taken) {
            bytes = bytes.min(refused.saturating_sub(1));
        }
        if bounds.refused.is_none_or(|refused| bounds.taken < refused) {
            bytes = bytes.max(bounds.taken);
        }
        RequestFit {
            bytes,
            learned: bounds.tokens_per_byte.is_some(),
        }
    })
}

/// The most input tokens per request byte a taken request reported here,
/// if any did: what prices a request before it is sent.
pub(crate) fn tokens_per_byte(endpoint: &str, model: &str, long: bool) -> Option<f64> {
    with(endpoint, model, long, |bounds| bounds.tokens_per_byte)
}

/// The window refused a request of `bytes` as too large.
pub(crate) fn refused(endpoint: &str, model: &str, long: bool, bytes: usize) {
    with(endpoint, model, long, |bounds| {
        bounds.refused = Some(bounds.refused.map_or(bytes, |known| known.min(bytes)));
    });
}

/// Whether the window is known to refuse a request of `bytes`.
pub(crate) fn refuses(endpoint: &str, model: &str, long: bool, bytes: usize) -> bool {
    with(endpoint, model, long, |bounds| {
        bytes > bounds.taken && bounds.refused.is_some_and(|refused| bytes >= refused)
    })
}

#[cfg(test)]
mod tests;
