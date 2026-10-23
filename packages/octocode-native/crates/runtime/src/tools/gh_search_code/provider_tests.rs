//! Provider-backed ghSearchCode tests, one child module per behavior, and their shared fixtures.
use super::{GhSearchCodeQuery, execute};
use crate::security::scan::Passthrough;
use crate::tools::gh_shared::test_support::{MemoryCache, default_context, one_shot_provider};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octocode_github::mock_http::Delayed;
use serde_json::json;
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn paths_of(out: &crate::response::result::ToolData) -> Vec<String> {
    out.data["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| row["path"].as_str().unwrap_or_default().to_owned())
        .collect()
}

mod backed;
mod empty;
mod owner_wide;
mod pages;
mod ref_commits;
mod requests;
mod window;
