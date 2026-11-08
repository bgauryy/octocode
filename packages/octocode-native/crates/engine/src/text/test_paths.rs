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
/// `x_test.exs`, Dart `x_test.dart`, C++ `x_unittest.cc`. Compared lowercase.
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
    {
        return true;
    }
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
mod tests {
    use super::is_test_path;

    #[cfg(unix)]
    #[test]
    fn a_backslash_in_a_unix_name_is_not_a_test_directory() {
        assert!(!is_test_path("tests\\a.ts"));
        assert!(!is_test_path("src/tests\\a.ts"));
        assert!(is_test_path("tests/a.ts"));
    }

    #[cfg(windows)]
    #[test]
    fn a_backslash_separates_windows_directories() {
        assert!(is_test_path("tests\\a.ts"));
        assert!(is_test_path("Tests\\Unit.cs"));
    }

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
            "Tests/Unit.cs",
            "Sources/AppTests/LoginTests.swift",
            "spec/models/user_spec.rb",
            "lib/user_spec.rb",
            "test/unit/test_user.rb",
            "core/src/WidgetSpec.scala",
            "lib/parser_test.exs",
            "lib/widget_test.dart",
            "base/strings_unittest.cc",
            "test.js",
            "test.ts",
            "test.mjs",
            "index.test-d.ts",
            "types/api.test-d.tsx",
        ] {
            assert!(is_test_path(path), "{path}");
        }
    }

    /// A `test_` stem is a Python/Ruby convention, not a Rust, Go or JS one:
    /// `test_code.rs` and its `test_code/` mount are production modules.
    #[test]
    fn a_test_prefix_is_a_convention_of_its_own_languages_only() {
        for path in [
            "src/tools/test_code.rs",
            "src/test_code/once_each.rs",
            "src/test_helpers.go",
            "lib/test_util.ts",
        ] {
            assert!(!is_test_path(path), "{path}");
        }
        for path in [
            "pkg/test_utils.py",
            "test_foo.py",
            "lib/test_user.rb",
            "tests/mod.rs",
        ] {
            assert!(is_test_path(path), "{path}");
        }
    }

    /// Ceedling/Unity, CMake test targets and shell suites spell `test_x.c`,
    /// `test_x.cpp`, `test_x.sh`.
    #[test]
    fn a_test_prefix_marks_c_family_and_shell_tests() {
        for path in [
            "src/test_parser.c",
            "src/test_parser.cc",
            "src/test_parser.cpp",
            "src/test_parser.cxx",
            "scripts/test_install.sh",
            "src/TEST_PARSER.CPP",
        ] {
            assert!(is_test_path(path), "{path}");
        }
        for path in ["src/test_parser.h", "src/test_parser.go", "src/test_.c"] {
            assert!(!is_test_path(path), "{path}");
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
            "src/testing.js",
            "src/latest.ts",
        ] {
            assert!(!is_test_path(path), "{path}");
        }
    }
}
