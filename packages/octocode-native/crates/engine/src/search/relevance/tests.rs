use super::*;

fn weight(line: &str, token: &str) -> u32 {
    line_weight(line.as_bytes(), line.find(token).expect("token in line"))
}

#[test]
fn declaration_names_outrank_code_which_outranks_prose() {
    for line in [
        "pub fn parse_config(path: &str) {",
        "    pub(crate) async fn parse_config() {",
        "export default function parse_config() {",
        "def parse_config(path):",
        "class parse_config:",
        "let mut parse_config = 1;",
        "#define parse_config 1",
    ] {
        assert_eq!(weight(line, "parse_config"), DECLARATION_WEIGHT, "{line}");
    }
    for line in [
        "let cfg = parse_config(path);",
        "impl Parser for parse_config {",
        "parse_config();",
        "#[derive(parse_config)]",
        "fn load(x: &'a str) -> Config { parse_config(x) }",
    ] {
        assert_eq!(weight(line, "parse_config"), CODE_WEIGHT, "{line}");
    }
    for line in [
        "// parse_config reads the file",
        "    /// Calls parse_config.",
        " * parse_config is here",
        "# parse_config in python",
        "-- parse_config in sql",
        "let label = \"parse_config\";",
        "let label = \"a \\\" parse_config\";",
        "let x = 1; // parse_config",
        "const s = `${a} parse_config`;",
    ] {
        assert_eq!(weight(line, "parse_config"), PROSE_WEIGHT, "{line}");
    }
}

/// C-family declarations name no keyword: modifiers, a type, then the
/// name (`public static <T> T firstNonNull(`, `private final long size =`).
/// A Go method's receiver sits between `func` and its name.
#[test]
fn typed_member_and_receiver_declarations_are_declarations() {
    for (line, token) in [
        (
            "  public static <T> T firstNonNull(@Nullable T first, @Nullable T second) {",
            "firstNonNull",
        ),
        (
            "  private final long maximumSize = UNSET_INT;",
            "maximumSize",
        ),
        ("    @Override public String toString() {", "toString"),
        (
            "static int parse_config(const char *path) {",
            "parse_config",
        ),
        ("  public CacheBuilder(Ticker ticker) {", "CacheBuilder"),
        ("  private readonly scene = new Scene();", "scene"),
        (
            "func (ng *Engine) exec(ctx context.Context) error {",
            "exec",
        ),
        ("func NewEngine(opts EngineOpts) *Engine {", "NewEngine"),
    ] {
        assert_eq!(weight(line, token), DECLARATION_WEIGHT, "{line}");
    }
    for (line, token) in [
        ("    return firstNonNull(a, b);", "firstNonNull"),
        ("  public void run() { firstNonNull(x); }", "firstNonNull"),
        (
            "  public static <T> T firstNonNull(@Nullable T first) {",
            "first)",
        ),
        ("    this.maximumSize = maximumSize;", "maximumSize"),
        (
            "func (ng *Engine) exec(ctx context.Context) error {",
            "Engine",
        ),
        ("  static_assert(parse_config(x));", "parse_config"),
    ] {
        assert_ne!(weight(line, token), DECLARATION_WEIGHT, "{line}");
    }
}

fn rank(line: &str, token: &str) -> u32 {
    line_rank(line.as_bytes(), line.find(token).expect("token in line"))
}

#[test]
fn hit_rank_puts_declarations_then_deciding_statements_before_plain_code_and_prose() {
    assert_eq!(rank("pub fn sample_limit() {", "sample_limit"), 3);
    for line in [
        "    this.maximumSize = maximumSize;",
        "\tz.SampleLimit = 0",
        "  long maximumSize = UNSET_INT;",
        "\tapp := appenderWithLimits(sl.sampleLimit)",
        "\tcase errors.Is(err, errSampleLimit):",
        "    } else if (maximumSize > 0) {",
        "        elif too_many_fields:",
        "\treturn 0, errSampleLimit",
        "            raise self.model.MultipleObjectsReturned(",
        "total += sample_limit;",
    ] {
        let token = [
            "maximumSize",
            "SampleLimit",
            "sampleLimit",
            "too_many_fields",
            "MultipleObjectsReturned",
            "sample_limit",
        ]
        .into_iter()
        .find(|token| line.contains(token))
        .expect("token");
        assert_eq!(rank(line, token), 2, "{line}");
    }
    for line in [
        "    checkArgument(maximumSize >= 0, \"negative\");",
        "        this.maximumSize == UNSET_INT,",
        "\tsl.metrics.targetScrapeSampleLimit.Inc()",
        "    builder.maximumSize(size, limit => limit + 1);",
        "    configure(maximumSize=10)",
        "\t\tsampleLimit: int(opts.limit),",
    ] {
        let token = ["maximumSize", "SampleLimit", "sampleLimit"]
            .into_iter()
            .find(|token| line.contains(token))
            .expect("token");
        assert_eq!(rank(line, token), 1, "{line}");
    }
    assert_eq!(rank("   * {@link #maximumSize(long)}", "maximumSize"), 0);
    assert_eq!(rank("x = 1 // maximumSize", "maximumSize"), 0);
}

#[test]
fn generated_files_are_named_by_path_or_header() {
    for path in [
        "src/generated/api.rs",
        "api.generated.ts",
        "a/b_pb2.py",
        "x.pb.go",
    ] {
        assert!(is_generated_path(path), "{path}");
    }
    for path in [
        "dist/index.js",
        "app.min.js",
        "tests/a.rs",
        "src/generator.rs",
    ] {
        assert!(!is_generated_path(path), "{path}");
    }
    for head in [
        "/* Automatically generated by generate-command-code.py, do not edit. */\n",
        "// Code generated by protoc-gen-go. DO NOT EDIT.\n\npackage x\n",
        "# @generated by tool\n",
        "#!/usr/bin/env python\n# -*- coding: utf-8 -*-\n# This file is autogenerated\n",
    ] {
        assert!(has_generated_header(head.as_bytes()), "{head}");
    }
    for head in [
        "fn main() {}\n",
        "// Generates the config table.\nfn generate() {}\n",
        "a\nb\nc\nd\ne\n// DO NOT EDIT below line five\n",
    ] {
        assert!(!has_generated_header(head.as_bytes()), "{head}");
    }
}

#[test]
fn test_generated_and_vendored_paths_are_demoted_below_the_root() {
    for path in [
        "tests/config.rs",
        "pkg/__tests__/a.ts",
        "config_test.go",
        "src/config.test.ts",
        "src/config.spec.js",
        "test_config.py",
        "src/generated/api.rs",
        "api.generated.ts",
        "dist/index.js",
        "vendor/lib/a.go",
        "app.min.js",
        "Tests\\Unit.cs",
    ] {
        assert!(is_demoted_path(path), "{path}");
    }
    for path in [
        "src/config.rs",
        "latest/config.rs",
        "contest.rs",
        "src/testing_utils.rs",
        "build.rs",
        "distance.rs",
    ] {
        assert!(!is_demoted_path(path), "{path}");
    }
}

/// A Jujutsu checkout is a repository like a git one: its
/// `.gitattributes` marks generated and vendored files.
#[test]
fn a_jj_checkout_applies_gitattributes() {
    let base = tempfile::tempdir().expect("tempdir");
    let checkout = base.path().join("checkout");
    std::fs::create_dir_all(checkout.join("src")).expect("dirs");
    std::fs::write(
        checkout.join(".gitattributes"),
        "src/gen.ts linguist-generated\nsrc/vendor.ts linguist-vendored\n",
    )
    .expect("attributes");
    assert!(GitAttributes::new(&checkout.join("src")).is_none());
    std::fs::create_dir(checkout.join(".jj")).expect("jj marker");
    let marks = GitAttributes::new(&checkout.join("src")).expect("a jj checkout");
    assert!(marks.marks_generated(&checkout.join("src/gen.ts")));
    assert!(marks.marks_generated(&checkout.join("src/vendor.ts")));
    assert!(!marks.marks_generated(&checkout.join("src/main.ts")));
}
