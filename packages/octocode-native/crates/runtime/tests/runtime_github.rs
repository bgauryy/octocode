//! The GitHub file, code, tree and repository tools through the runtime:
//! reads, recoveries, rate limits and bulk dispatch against a mock GitHub.

use crate::support::{self, Run as _};
use octocode_contract_types::ErrorCode;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use octocode_github::mock_http::{Arrivals, Delayed};
use serde_json::json;
use std::time::Duration;
use support::{ALL_EXTRAS, Workspace, call, checks, row_data, row_status};
use wiremock::matchers::{method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod file_reads;
mod missing_targets;
mod refs;
mod failures;
mod materialize;
mod batches;
