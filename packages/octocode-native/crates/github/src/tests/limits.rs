//! Primary and secondary rate limits, spacing, the circuit breaker and the
//! code-search window, one behavior per child module.
use super::*;

mod circuit;
mod code_search_window;
mod primary;
mod secondary;
mod spacing;

fn secondary_limit_response(retry_after: Option<&str>) -> ResponseTemplate {
    let mut response = ResponseTemplate::new(403)
        .insert_header("x-ratelimit-remaining", "4000")
        .insert_header("x-ratelimit-reset", (epoch_secs() + 3600).to_string().as_str())
        .insert_header("x-ratelimit-resource", "search")
        .set_body_json(serde_json::json!({
            "message": "You have exceeded a secondary rate limit. Please wait a few minutes before you try again."
        }));
    if let Some(retry_after) = retry_after {
        response = response.insert_header("retry-after", retry_after);
    }
    response
}
