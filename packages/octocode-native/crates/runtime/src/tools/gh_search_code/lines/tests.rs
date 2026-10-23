use super::budget::{SCAN_BUDGET_BYTES, ScanBudget};
use super::*;

use crate::security::scan::Passthrough;

mod anchors;
mod budget;
mod listing;

fn test_context() -> RequestContext {
    RequestContext::with_timeout(std::time::Duration::from_secs(30), MAX_SCAN_BYTES)
}
