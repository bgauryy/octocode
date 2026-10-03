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
/// `a.spec.js`, `a.e2e.ts`), compared lowercase.
const TEST_NAME_MARKERS: &[&str] = &[".test.", ".spec.", ".e2e."];

/// Stem suffixes of snake-case test files in any language: Go `x_test.go`,
/// Rust `x_test.rs`, Python `x_test.py`, Ruby `x_spec.rb`, Elixir
/// `x_test.exs`, Dart `x_test.dart`, C++ `x_unittest.cc`. Compared lowercase.
const TEST_STEM_SUFFIXES: &[&str] = &["_test", "_tests", "_spec", "_unittest"];

/// Stem prefixes of test files (pytest and minitest `test_x.py`, `test_x.rb`).
const TEST_STEM_PREFIXES: &[&str] = &["test_"];

/// Whole file names that hold tests (pytest `conftest.py`, Rust `tests.rs`).
const TEST_FILE_NAMES: &[&str] = &["conftest.py", "tests.rs", "test.rs"];

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

/// Whether `path` (repository- or search-root-relative, `/` or `\`
/// separated) is a test file or lives in a test directory.
#[must_use]
pub fn is_test_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    let mut segments = normalized.rsplit('/');
    let name = segments.next().unwrap_or_default();
    if segments.any(|dir| {
        TEST_DIRECTORIES
            .iter()
            .any(|test| dir.eq_ignore_ascii_case(test))
    }) {
        return true;
    }
    is_test_file_name(name)
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
    let lower_stem = stem.to_ascii_lowercase();
    if TEST_STEM_SUFFIXES
        .iter()
        .any(|suffix| lower_stem.len() > suffix.len() && lower_stem.ends_with(suffix))
        || TEST_STEM_PREFIXES
            .iter()
            .any(|prefix| lower_stem.len() > prefix.len() && lower_stem.starts_with(prefix))
    {
        return true;
    }
    let extension = extension.to_ascii_lowercase();
    TEST_CLASS_SUFFIXES
        .iter()
        .filter(|(extensions, _)| extensions.contains(&extension.as_str()))
        .flat_map(|(_, suffixes)| suffixes.iter())
        .any(|suffix| stem.len() > suffix.len() && stem.ends_with(suffix))
}

#[cfg(test)]
mod tests {
    use super::is_test_path;

    #[test]
    fn recognizes_each_language_convention() {
        for path in [
            "src/__tests__/a.ts",
            "src/a.test.ts",
            "src/a.spec.js",
            "e2e/login.ts",
            "pkg/cmd/discussion/client/client_test.go",
            "internal/x/testdata/input.go",
            "tests/integration.rs",
            "src/parser/tests.rs",
            "src/lexer_test.rs",
            "tests/test_api.py",
            "pkg/test_utils.py",
            "pkg/api_test.py",
            "pkg/conftest.py",
            "src/test/java/com/acme/Widget.java",
            "src/main/java/com/acme/WidgetTest.java",
            "app/src/androidTest/java/A.kt",
            "Tests\\Unit.cs",
            "Sources/AppTests/LoginTests.swift",
            "spec/models/user_spec.rb",
            "lib/user_spec.rb",
            "test/unit/test_user.rb",
            "core/src/WidgetSpec.scala",
            "lib/parser_test.exs",
            "lib/widget_test.dart",
            "base/strings_unittest.cc",
        ] {
            assert!(is_test_path(path), "{path}");
        }
    }

    #[test]
    fn leaves_source_alone() {
        for path in [
            "pkg/cmd/discussion/create/create.go",
            "src/main/java/com/acme/Latest.java",
            "src/contest.py",
            "src/testing_utils.go",
            "lib/response.js",
            "src/attestation.rs",
            "src/protest_spec_helper/README.md",
            "_test.go",
            "test_.py",
        ] {
            assert!(!is_test_path(path), "{path}");
        }
    }
}
