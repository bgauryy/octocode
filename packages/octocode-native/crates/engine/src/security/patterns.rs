//! Canonical ordered secret-detection patterns: plain data, also read by the
//! engine build script to derive the detector's prescan gate. Each child
//! module holds one contiguous slice of the order; [`PATTERNS`] joins them.

mod ai_and_analytics;
mod auth_and_cloud;
mod dev_tooling;
mod git_hosting;
mod keys_and_databases;
mod observability_and_payments;
mod platforms;
mod workspace_and_messaging;

#[derive(Debug, Clone, Copy)]
pub struct SecretPattern {
    pub name: &'static str,
    pub file_context: Option<&'static str>,
    pub regex: &'static str,
}

/// Secret patterns in evaluation order; a pattern's index is its identity.
pub static PATTERNS: &[SecretPattern] = &join::<{ total(&SLICES) }>(&SLICES);

/// The slices of [`PATTERNS`], in evaluation order.
const SLICES: [&[SecretPattern]; 8] = [
    ai_and_analytics::PATTERNS,
    auth_and_cloud::PATTERNS,
    platforms::PATTERNS,
    keys_and_databases::PATTERNS,
    dev_tooling::PATTERNS,
    observability_and_payments::PATTERNS,
    workspace_and_messaging::PATTERNS,
    git_hosting::PATTERNS,
];

/// The number of patterns in `slices`.
const fn total(slices: &[&[SecretPattern]]) -> usize {
    let mut count = 0;
    let mut index = 0;
    while index < slices.len() {
        count += slices[index].len();
        index += 1;
    }
    count
}

/// `slices` joined in order into one table of exactly `N` patterns.
const fn join<const N: usize>(slices: &[&[SecretPattern]]) -> [SecretPattern; N] {
    let mut joined = [SecretPattern {
        name: "",
        file_context: None,
        regex: "",
    }; N];
    let mut filled = 0;
    let mut slice = 0;
    while slice < slices.len() {
        let mut index = 0;
        while index < slices[slice].len() {
            joined[filled] = slices[slice][index];
            filled += 1;
            index += 1;
        }
        slice += 1;
    }
    assert!(filled == N, "pattern count mismatch");
    joined
}
