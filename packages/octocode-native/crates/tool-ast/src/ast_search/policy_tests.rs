//! astSearch scope: path policy, walk disclosure, cancellation and
//! grammar selection.
use super::test_rows::*;
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_runtime_base::test_support::{
    AfterRoot, Fixture, checks, sensitive_fixture, simple_policy, skipped_reasons,
};
use serde_json::json;

mod grammar_windows;
mod languages;
mod walk;
