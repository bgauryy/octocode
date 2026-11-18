//! Whether a repository-relative path holds support code: examples,
//! benchmarks, stories, test helpers and test setup. Support code is neither
//! production nor a test by the one test-path rule
//! ([`super::test_paths::is_test_path`]); the code graph names it `support`
//! and keeps it out of production findings.

/// Directory names whose files support the project without shipping in it,
/// compared lowercase.
const SUPPORT_DIRECTORIES: &[&str] = &["testing", "examples", "example", "benches", "benchmarks"];

/// File-name fragments of benchmark and Storybook files, compared lowercase.
const SUPPORT_NAME_MARKERS: &[&str] = &[".bench.", ".stories."];

/// File-name prefixes of test setup files (`setupTests.ts`,
/// `jest.setup.js`, `vitest.setup.ts`), compared lowercase.
const SUPPORT_NAME_PREFIXES: &[&str] = &[
    "setuptests.",
    "jest.setup",
    "vitest.setup",
    "test-setup.",
    "testsetup.",
];

/// Whether `path` (separated as for [`super::test_paths::is_test_path`]) is
/// support code. A test by the test-path rule is never support.
#[must_use]
pub fn is_support_path(path: &str) -> bool {
    if super::test_paths::is_test_path(path) {
        return false;
    }
    let mut segments = path.rsplit(std::path::is_separator);
    let name = segments.next().unwrap_or_default().to_ascii_lowercase();
    segments.any(|dir| {
        SUPPORT_DIRECTORIES
            .iter()
            .any(|support| dir.eq_ignore_ascii_case(support))
    }) || SUPPORT_NAME_MARKERS
        .iter()
        .any(|marker| name.contains(marker))
        || SUPPORT_NAME_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
}

#[cfg(test)]
mod tests;
