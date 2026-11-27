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
        "src/tools/process_tests/importers.rs",
        "src/search/match_tests/source_limit.rs",
        "pkg/widget_test/helpers.go",
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
