use super::is_support_path;

#[test]
fn recognizes_support_code() {
    for path in [
        "examples/demo.rs",
        "crates/cli/Examples/basic.ts",
        "benches/parse.rs",
        "benchmarks/run.py",
        "testing/helper.go",
        "src/parse.bench.ts",
        "src/Button.stories.tsx",
        "src/setupTests.ts",
        "jest.setup.js",
        "vitest.setup.ts",
    ] {
        assert!(is_support_path(path), "{path}");
    }
}

#[test]
fn leaves_tests_and_source_alone() {
    for path in [
        "examples/tests/demo.rs",
        "benches/speed.test.ts",
        "src/lib.rs",
        "src/example.rs",
        "src/benchmark.go",
        "src/testing.js",
    ] {
        assert!(!is_support_path(path), "{path}");
    }
}
