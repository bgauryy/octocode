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
//! request a source split into sections can send ([`request_bytes`]). They
//! also bound the provider's own prompt, which no request body shows, so a
//! request is priced at most what it can cost ([`prompt_tokens_at_most`]).
use std::collections::{BTreeMap, HashMap};
use std::sync::{LazyLock, Mutex, PoisonError};

#[derive(Clone, Default)]
struct Bounds {
    /// The largest request taken.
    taken: usize,
    /// The smallest request refused as too large.
    refused: Option<usize>,
    /// The most input tokens per request byte a taken request reported.
    tokens_per_byte: Option<f64>,
    /// By question count, the fewest input tokens a taken request asking
    /// that many questions reported ([`prompt_tokens_at_most`]).
    least_tokens: BTreeMap<usize, u64>,
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

/// The window took a request of `bytes` asking `questions` questions,
/// reported as `input_tokens`.
pub(crate) fn took(
    endpoint: &str,
    model: &str,
    long: bool,
    bytes: usize,
    questions: usize,
    input_tokens: Option<u64>,
) {
    with(endpoint, model, long, |bounds| {
        bounds.taken = bounds.taken.max(bytes);
        if let Some(tokens) = input_tokens.filter(|_| questions > 0) {
            let least = bounds.least_tokens.entry(questions).or_insert(tokens);
            *least = (*least).min(tokens);
        }
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

/// The most input tokens the provider's own prompt can add to a request
/// asking `questions` questions: what no request body shows, so a request is
/// priced at most its body bytes (a token covers at least one byte of the
/// text sent) plus this. A taken request's reported tokens `T` are its prompt
/// and its body's, so its prompt is at most `T`; a prompt that costs a fixed
/// part per request and per question is then at most `T` for as many
/// questions or fewer, and `T` scaled by the ratio for more. The least such
/// bound over the requests taken here; `None` before any was taken, when
/// nothing bounds it.
pub(crate) fn prompt_tokens_at_most(
    endpoint: &str,
    model: &str,
    long: bool,
    questions: usize,
) -> Option<u64> {
    with(endpoint, model, long, |bounds| {
        bounds
            .least_tokens
            .iter()
            .map(|(&asked, &tokens)| {
                if questions <= asked {
                    tokens
                } else {
                    let (questions, asked) = (
                        u64::try_from(questions).unwrap_or(u64::MAX),
                        u64::try_from(asked).unwrap_or(u64::MAX),
                    );
                    tokens.saturating_mul(questions).div_ceil(asked)
                }
            })
            .min()
    })
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
