//! Whether a repository-relative path holds tests, by the directory layouts
//! and file-naming conventions of common languages. This is the one test-path
//! rule: review ranking, search relevance, graph roles, and confidence labels
//! all ask it, so a convention added here applies everywhere.

/// Directory names whose files are tests or test inputs in any language
/// (Maven/Gradle `src/test/`, Rust `tests/`, Jest `__tests__/`, RSpec `spec/`,
/// Go `testdata/`, Android `androidTest/`), compared lowercase.
const TEST_DIRECTORIES: &[&str] = &[
    "test",
    "tests",
    "__tests__",
    "__mocks__",
    "spec",
    "specs",
    "testdata",
    "fixtures",
    "__fixtures__",
    "e2e",
    "androidtest",
];

/// File-name fragments of JavaScript/TypeScript test files (`a.test.ts`,
/// `a.spec.js`, `a.e2e.ts`, tsd type tests `index.test-d.ts`), compared
/// lowercase.
const TEST_NAME_MARKERS: &[&str] = &[".test.", ".spec.", ".e2e.", ".test-d."];

/// Stem suffixes of snake-case test files in any language: Go `x_test.go`,
/// Rust `x_test.rs`, Python `x_test.py`, Ruby `x_spec.rb`, Elixir
/// `x_test.exs`, Dart `x_test.dart`, C++ `x_unittest.cc`; and of the
/// directories that hold such a module's children (Rust `x_tests/`).
/// Compared lowercase.
const TEST_STEM_SUFFIXES: &[&str] = &["_test", "_tests", "_spec", "_unittest"];

/// Stem prefixes of test files, per extension: pytest/unittest `test_x.py`
/// and minitest `test_x.rb`, Ceedling/Unity and CMake `test_x.c` / `test_x.cpp`,
/// shell suites `test_x.sh`. Other languages do not spell tests this way (Go
/// and Rust use suffixes or `tests/`), so a Rust `test_code.rs` is source.
const TEST_STEM_PREFIXES: &[(&[&str], &str)] =
    &[(&["py", "pyi", "rb", "c", "cc", "cpp", "cxx", "sh"], "test_")];

/// Whole file names that hold tests (pytest `conftest.py`, Rust `tests.rs`,
/// a single-file JavaScript/TypeScript package's `test.js`).
const TEST_FILE_NAMES: &[&str] = &[
    "conftest.py",
    "tests.rs",
    "test.rs",
    "test.js",
    "test.mjs",
    "test.cjs",
    "test.jsx",
    "test.ts",
    "test.mts",
    "test.cts",
    "test.tsx",
];

/// Case-sensitive class-name suffixes per extension: JUnit `FooTest.java`,
/// xUnit `FooTests.cs`, XCTest `FooTests.swift`, PHPUnit `FooTest.php`,
/// Spock/ScalaTest/Kotest `FooSpec.groovy`. Case matters: `Latest.java`
/// is not a test.
const TEST_CLASS_SUFFIXES: &[(&[&str], &[&str])] = &[
    (
        &["java", "kt", "cs", "fs", "vb", "swift", "php", "m", "mm"],
        &["Test", "Tests"],
    ),
    (&["scala", "groovy"], &["Test", "Tests", "Spec"]),
];

/// Whether `path` (repository- or search-root-relative, separated by the
/// platform's path separators: `/`, and `\` too on Windows) is a test file
/// or lives in a test directory. On Unix `\` is a file-name byte, so
/// `tests\a.ts` is one name, not a file in `tests`.
#[must_use]
pub fn is_test_path(path: &str) -> bool {
    let mut segments = path.rsplit(std::path::is_separator);
    let name = segments.next().unwrap_or_default();
    if segments.any(|dir| {
        TEST_DIRECTORIES
            .iter()
            .any(|test| dir.eq_ignore_ascii_case(test))
            || has_test_stem_suffix(dir)
    }) {
        return true;
    }
    is_test_file_name(name)
}

/// Whether a name (a file stem, or a directory: a Rust module `x_tests.rs`
/// and its child directory `x_tests/` are one module) ends in a
/// [`TEST_STEM_SUFFIXES`] suffix after a non-empty stem.
fn has_test_stem_suffix(stem: &str) -> bool {
    let lower = stem.to_ascii_lowercase();
    TEST_STEM_SUFFIXES
        .iter()
        .any(|suffix| lower.len() > suffix.len() && lower.ends_with(suffix))
}

fn is_test_file_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if TEST_FILE_NAMES.contains(&lower.as_str())
        || TEST_NAME_MARKERS
            .iter()
            .any(|marker| lower.contains(marker))
    {
        return true;
    }
    let Some((stem, extension)) = name.rsplit_once('.') else {
        return false;
    };
    if has_test_stem_suffix(stem) {
        return true;
    }
    let lower_stem = stem.to_ascii_lowercase();
    let extension = extension.to_ascii_lowercase();
    if TEST_STEM_PREFIXES.iter().any(|(extensions, prefix)| {
        extensions.contains(&extension.as_str())
            && lower_stem.len() > prefix.len()
            && lower_stem.starts_with(prefix)
    }) {
        return true;
    }
    TEST_CLASS_SUFFIXES
        .iter()
        .filter(|(extensions, _)| extensions.contains(&extension.as_str()))
        .flat_map(|(_, suffixes)| suffixes.iter())
        .any(|suffix| stem.len() > suffix.len() && stem.ends_with(suffix))
}

#[cfg(test)]
mod tests;
