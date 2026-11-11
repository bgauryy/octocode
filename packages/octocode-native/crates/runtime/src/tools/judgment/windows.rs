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
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, PoisonError};

#[derive(Clone, Copy, Default)]
struct Bounds {
    /// The largest request taken.
    taken: usize,
    /// The smallest request refused as too large.
    refused: Option<usize>,
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

/// The window took a request of `bytes`.
pub(crate) fn took(endpoint: &str, model: &str, long: bool, bytes: usize) {
    with(endpoint, model, long, |bounds| {
        bounds.taken = bounds.taken.max(bytes);
    });
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
mod tests {
    use super::*;

    #[test]
    fn a_window_is_known_only_past_its_smallest_refusal_and_largest_take() {
        let (endpoint, model) = ("http://windows.test/a", "m");
        assert!(!refuses(endpoint, model, false, 1_000_000));
        took(endpoint, model, false, 100);
        refused(endpoint, model, false, 900);
        refused(endpoint, model, false, 500);
        // Between the bounds the provider is asked.
        assert!(!refuses(endpoint, model, false, 499));
        assert!(refuses(endpoint, model, false, 500));
        assert!(refuses(endpoint, model, false, 2_000));
        // Another window, model or endpoint knows nothing of it.
        assert!(!refuses(endpoint, model, true, 2_000));
        assert!(!refuses(endpoint, "other", false, 2_000));
        assert!(!refuses("http://windows.test/b", model, false, 2_000));
    }

    /// A sparser text the provider takes past the refused size lifts the
    /// bound: sizes up to it are asked again.
    #[test]
    fn a_larger_take_lifts_a_refusal() {
        let (endpoint, model) = ("http://windows.test/c", "m");
        refused(endpoint, model, false, 500);
        took(endpoint, model, false, 800);
        assert!(!refuses(endpoint, model, false, 700));
        assert!(!refuses(endpoint, model, false, 800));
        assert!(refuses(endpoint, model, false, 801));
    }
}
