use super::*;

#[test]
fn a_window_is_known_only_past_its_smallest_refusal_and_largest_take() {
    let (endpoint, model) = ("http://windows.test/a", "m");
    assert!(!refuses(endpoint, model, false, 1_000_000));
    took(endpoint, model, false, 100, 1, None);
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
    took(endpoint, model, false, 800, 1, None);
    assert!(!refuses(endpoint, model, false, 700));
    assert!(!refuses(endpoint, model, false, 800));
    assert!(refuses(endpoint, model, false, 801));
}

/// Unseen, a window holds one byte per stated token; a reported token
/// count sizes it by the densest text taken, below the smallest refusal
/// and never under the largest take.
#[test]
fn a_request_size_follows_the_stated_window_at_the_learned_density() {
    let (endpoint, model) = ("http://windows.test/d", "m");
    let fit = |stated| request_bytes(endpoint, model, false, stated);
    assert_eq!(
        fit(1_000),
        RequestFit {
            bytes: 1_000,
            learned: false
        }
    );
    took(endpoint, model, false, 400, 1, Some(100));
    assert_eq!(
        fit(1_000),
        RequestFit {
            bytes: 4_000,
            learned: true
        }
    );
    // A denser text taken later is what the size answers to.
    took(endpoint, model, false, 300, 1, Some(150));
    assert_eq!(fit(1_000).bytes, 2_000);
    refused(endpoint, model, false, 1_500);
    assert_eq!(fit(1_000).bytes, 1_499);
    took(endpoint, model, false, 1_200, 1, None);
    assert_eq!(fit(10).bytes, 1_200);
    // The long window is its own.
    assert!(!request_bytes(endpoint, model, true, 1_000).learned);
}

/// Nothing bounds the provider's own prompt before a request was taken; a
/// taken request's reported tokens bound it for as many questions or fewer,
/// scaled by the ratio for more, and the least bound holds.
#[test]
fn the_providers_prompt_is_bounded_only_by_requests_taken() {
    let (endpoint, model) = ("http://windows.test/prompt", "m");
    assert_eq!(prompt_tokens_at_most(endpoint, model, false, 1), None);
    took(endpoint, model, false, 600, 2, Some(900));
    assert_eq!(prompt_tokens_at_most(endpoint, model, false, 1), Some(900));
    assert_eq!(prompt_tokens_at_most(endpoint, model, false, 2), Some(900));
    assert_eq!(
        prompt_tokens_at_most(endpoint, model, false, 5),
        Some(2_250)
    );
    took(endpoint, model, false, 9_000, 10, Some(4_000));
    assert_eq!(
        prompt_tokens_at_most(endpoint, model, false, 5),
        Some(2_250)
    );
    assert_eq!(
        prompt_tokens_at_most(endpoint, model, false, 20),
        Some(8_000)
    );
    took(endpoint, model, false, 200, 2, Some(700));
    assert_eq!(prompt_tokens_at_most(endpoint, model, false, 2), Some(700));
    // Another window knows nothing of it.
    assert_eq!(prompt_tokens_at_most(endpoint, model, true, 1), None);
}
