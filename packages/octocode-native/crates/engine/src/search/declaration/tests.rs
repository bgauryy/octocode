use super::*;

fn rank(line: &str, token: &str) -> u32 {
    line_rank(line.as_bytes(), line.find(token).expect("token in line"))
}

#[test]
fn a_declared_name_ranks_as_a_declaration_and_code_or_prose_does_not() {
    for line in [
        "pub fn parse_config(path: &str) {",
        "    pub(crate) async fn parse_config() {",
        "export default function parse_config() {",
        "def parse_config(path):",
        "class parse_config:",
        "let mut parse_config = 1;",
        "#define parse_config 1",
    ] {
        assert_eq!(rank(line, "parse_config"), DECLARATION_RANK, "{line}");
    }
    for line in [
        "let cfg = parse_config(path);",
        "impl Parser for parse_config {",
        "parse_config();",
        "#[derive(parse_config)]",
        "fn load(x: &'a str) -> Config { parse_config(x) }",
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
        assert_eq!(rank(line, "parse_config"), 0, "{line}");
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
        assert_eq!(rank(line, token), DECLARATION_RANK, "{line}");
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
        assert_ne!(rank(line, token), DECLARATION_RANK, "{line}");
    }
}

/// A search that is a declaration head names its last word; the walk that
/// marks a hit line `declaration` decides, so modifiers, annotations, `mut`
/// and a Go receiver lead to the name as they do on a source line.
#[test]
fn a_declaration_head_names_its_last_word() {
    for (head, name) in [
        ("fn merge", "merge"),
        ("pub fn push_warnings", "push_warnings"),
        ("pub(crate) fn walk_builder", "walk_builder"),
        ("pub async fn run", "run"),
        ("export default class Merge", "Merge"),
        ("async def handler", "handler"),
        ("let mut limit", "limit"),
        ("@Override public String toString", "toString"),
        ("func (r *Engine) exec", "exec"),
        ("func (Engine) exec", "exec"),
        ("#define MAX_DEPTH", "MAX_DEPTH"),
        ("  fn  spaced  ", "spaced"),
    ] {
        assert_eq!(declared_name(head), Some(name), "{head}");
        // The same text on a source line is the declaration the search names.
        assert_eq!(rank(head, name), DECLARATION_RANK, "{head}");
    }
    // A head written up to its signature's opening names the identifier its
    // last word starts with.
    for (head, name) in [
        ("pub fn walk_builder(", "walk_builder"),
        ("fn apply<T>(", "apply"),
        ("def handler(self", "handler"),
    ] {
        assert_eq!(declared_name(head), Some(name), "{head}");
    }
    for text in [
        "merge",
        "pub fn",
        "foo bar",
        "pub use merge",
        "let x = 1",
        "class Foo extends Bar",
        "impl Parser for Config",
        "return merge",
        "",
    ] {
        assert_eq!(declared_name(text), None, "{text}");
    }
}
