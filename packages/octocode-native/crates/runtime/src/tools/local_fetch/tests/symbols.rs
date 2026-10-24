//! The symbols view: outlines, redactions, gutters, spans and the cache.

use super::*;
use octocode_contract_types::ReadView;

/// A source whose earlier redaction (a whole-file key block) broke the
/// parse names those lines as redacted text, never as syntax the grammar
/// lacks.
#[test]
fn a_symbols_outline_names_lines_a_redaction_left_unparseable() {
    let store = crate::cache::Store::new(crate::cache::CacheConfig::default(), None);
    let source = "pub fn a() -> u8 {\n    let key = 1;\n[REDACTED-PRIVATEKEYFRAGMENT]\n    key\n}\n\npub fn b() {}\n";
    let view = crate::tools::source_view::outline::symbols_view(
        &store,
        source,
        "k.rs",
        Path::new("/fixture/k.rs"),
        &crate::security::ContentSecurity::new(),
    );
    let warning = view.warning.clone().expect("recovered parse");
    assert!(
        warning.contains("secret-shaped text was redacted"),
        "{warning}"
    );
    assert!(!warning.contains("grammar lacks"), "{warning}");
}

/// A symbols view is kept in memory and on disk across calls, so it renders
/// from the redacted source: neither tier, nor the outline, holds a secret
/// of the file, and the view names the line its render redacted.
#[test]
fn a_kept_symbols_view_never_holds_a_secret_of_the_source() {
    let dir = tempfile::tempdir().expect("dir");
    let store = crate::cache::Store::new(
        crate::cache::CacheConfig::default(),
        Some(dir.path().to_owned()),
    );
    let token = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
    let source = format!("pub const GH: &str = \"{token}\";\n\npub fn run() -> u8 {{\n    1\n}}\n");
    let security = crate::security::ContentSecurity::new();
    let view = crate::tools::source_view::outline::symbols_view(
        &store,
        &source,
        "sec.rs",
        Path::new("/fixture/sec.rs"),
        &security,
    );
    let outline = view.outline.clone().expect("outline");
    assert!(outline.contains("[REDACTED"), "{outline}");
    assert!(!outline.contains(token), "{outline}");
    assert_eq!(view.redacted_lines, vec![(1, 1)]);
    let mut kept = 0;
    for entry in fs::read_dir(dir.path()).expect("cache dir") {
        let path = entry.expect("entry").path();
        if path
            .extension()
            .is_some_and(|extension| extension == "entry")
        {
            let text = fs::read_to_string(&path).expect("cache file");
            assert!(!text.contains("ghp_"), "{}: {text}", path.display());
            kept += 1;
        }
    }
    assert_eq!(kept, 1, "the view is on disk");
}

/// An outline page counts the placeholders its render put in, as a plain
/// page does.
#[test]
fn a_symbols_page_counts_the_secrets_its_outline_redacted() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let p = t.0.join("sec.ts");
    fs::write(
        &p,
        "export const KEY = 'SECRET';\n\nexport function run(): number {\n  return 1;\n}\n",
    )
    .expect("fixture");
    let row = fetch(&qj(&p, serde_json::json!({"view":"symbols"})), &paths);
    let content = row.content.expect("content");
    assert!(!content.contains("SECRET"), "{content}");
    assert!(content.contains("[REDACTED]"), "{content}");
    assert!(
        row.warnings
            .iter()
            .any(|warning| warning.starts_with("redactedContent: 1 ")),
        "{:?}",
        row.warnings
    );
}

/// `view:"symbols"` uses one gutter, `N\t`, in every language and on
/// every line, closing-brace lines included.
#[test]
fn symbols_view_uses_a_tab_gutter_everywhere() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let p = t.0.join("b.ts");
    fs::write(
            &p,
            "import { a } from 'a';\nimport { b } from 'b';\n\nexport class Box {\n  open(): void {\n    a();\n  }\n}\n\nexport function make(): Box {\n  return new Box();\n}\n",
        )
        .expect("fixture");
    let content = fetch(&qj(&p, serde_json::json!({"view":"symbols"})), &paths)
        .content
        .expect("content");
    assert!(!content.is_empty());
    for line in content.lines() {
        let (number, _) = line
            .split_once('\t')
            .unwrap_or_else(|| panic!("tab gutter: {line:?} in {content:?}"));
        // A declaration head reads `start-end`; every other line one number.
        let mut bounds = number.splitn(2, '-');
        assert!(
            bounds.all(|bound| bound.parse::<u64>().is_ok()),
            "{line:?} in {content:?}"
        );
    }
}

/// The outline's lines are the source's own text: spaces between tokens
/// (`): boolean {`, `a: string, ...rest`) and a member's indentation stay.
#[test]
fn symbols_view_keeps_the_sources_spacing() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let p = t.0.join("s.ts");
    let source = "export function isBlank(str: string, ...rest: any[]): boolean {\n  return !str;\n}\n\nexport class Box {\n  open(a: string, b: number): void {\n    a();\n    b();\n  }\n  close(): void {\n    a();\n    b();\n  }\n}\n";
    fs::write(&p, source).expect("fixture");
    let content = fetch(&qj(&p, serde_json::json!({"view":"symbols"})), &paths)
        .content
        .expect("content");
    for text in [
        "export function isBlank(str: string, ...rest: any[]): boolean {",
        "export class Box {",
        "  open(a: string, b: number): void {",
    ] {
        assert!(
            content.lines().any(|line| line.ends_with(text)),
            "{text:?} in {content:?}"
        );
    }
}

/// A symbols read of a data document outlines its keys; a file with no
/// outline names the specific reason.
#[test]
fn symbols_outline_data_keys_and_name_why_an_outline_is_missing() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let json = t.0.join("data.json");
    let mut body = String::from("{\n  \"name\": \"x\",\n  \"items\": {\n");
    body.push_str(&"    \"k\": 1,\n".repeat(200));
    body.push_str("    \"last\": 2\n  }\n}\n");
    fs::write(&json, &body).expect("fixture");
    let r = fetch(&qj(&json, serde_json::json!({"view": "symbols"})), &paths);
    assert_eq!(r.view, Some(ReadView::Symbols), "{r:?}");
    assert_eq!(
        r.content.as_deref(),
        Some("2\t  \"name\": \"x\",\n3-205\t  \"items\": {\n")
    );
    let text = t.0.join("notes.txt");
    fs::write(&text, "plain\n".repeat(100)).expect("fixture");
    let r = fetch(&qj(&text, serde_json::json!({"view": "symbols"})), &paths);
    assert_eq!(
        r.view_fallback
            .as_ref()
            .map(|fallback| fallback.reason.as_str()),
        Some("unsupported-language"),
        "{r:?}"
    );
}

/// A symbols outline from a recovered parse (Flow types) says which lines
/// the parser skipped, as a block read does.
#[test]
fn a_symbols_outline_from_a_recovered_parse_names_the_skipped_lines() {
    let t = Fixture::new();
    let p = t.0.join("Hooks.js");
    let mut flow = String::from("// @flow\ntype Dispatch<A> = A => void;\n");
    for i in 0..20 {
        flow.push_str(&format!(
            "export function useState{i}<S>(init: S): [S, Dispatch<S>] {{\n  const value = init;\n  const other = value;\n  return [other, () => {{}}];\n}}\n"
        ));
    }
    fs::write(&p, &flow).expect("fixture");
    let r = fetch(
        &qj(&p, serde_json::json!({"view": "symbols"})),
        &Paths(t.0.clone()),
    );
    assert_eq!(r.view, Some(ReadView::Symbols), "{r:?}");
    assert!(
        r.warnings
            .iter()
            .any(|warning| warning.starts_with("The parser could not parse lines")),
        "{:?}",
        r.warnings
    );
    // A later outline page does not repeat it.
    let later = fetch(
        &qj(
            &p,
            serde_json::json!({"view": "symbols", "readProgress":{"unit":"lines","offset":5}}),
        ),
        &Paths(t.0.clone()),
    );
    assert_eq!(later.view, Some(ReadView::Symbols), "{later:?}");
    assert!(
        !later
            .warnings
            .iter()
            .any(|warning| warning.starts_with("The parser could not parse lines")),
        "{:?}",
        later.warnings
    );
}

/// A kept symbols view is served without scanning its source again.
#[test]
fn a_kept_symbols_view_is_served_without_a_rescan() {
    struct Counting(AtomicUsize);
    impl ContentScan for Counting {
        fn sanitize(
            &self,
            text: &str,
            _: &Path,
        ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok((text.to_owned(), vec![]))
        }
    }
    let store = crate::cache::Store::new(crate::cache::CacheConfig::default(), None);
    let scan = Counting(AtomicUsize::new(0));
    let source = "pub fn run() -> u8 {\n    1\n}\n";
    let view = |scan: &Counting| {
        crate::tools::source_view::outline::symbols_view(
            &store,
            source,
            "run.rs",
            Path::new("/fixture/run.rs"),
            scan,
        )
    };
    let first = view(&scan);
    let scans = scan.0.load(Ordering::SeqCst);
    assert!(scans > 0);
    let again = view(&scan);
    assert!(std::sync::Arc::ptr_eq(&first, &again));
    assert_eq!(scan.0.load(Ordering::SeqCst), scans, "a hit scans nothing");
}

/// A symbols outline names each multi-line declaration's line span.
#[test]
fn symbols_view_heads_carry_line_ranges() {
    let t = Fixture::new();
    let p = t.0.join("m.py");
    fs::write(&p, PY_TWO).expect("fixture");
    let content = fetch(
        &qj(&p, serde_json::json!({"view": "symbols"})),
        &Paths(t.0.clone()),
    )
    .content
    .expect("content");
    assert!(content.contains("3-5\tdef first(a):"), "{content}");
    assert!(content.contains("8-11\tdef second(b):"), "{content}");
    assert!(content.contains("1\timport os"), "{content}");
}

/// A declaration that starts with annotations or decorators carries its
/// span on its head row (the class, method or field line); the collapsed
/// annotation row above it stays a plain row, in Java as in TypeScript.
#[test]
fn symbols_view_spans_sit_on_the_head_row_below_annotations() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let outline = |name: &str, text: &str| {
        let p = t.0.join(name);
        fs::write(&p, text).expect("fixture");
        fetch(&qj(&p, serde_json::json!({"view": "symbols"})), &paths)
            .content
            .expect("content")
    };
    let java = outline(
        "Ann.java",
        "package x;\n\n@Entity\n@Table(name = \"t\")\npublic class Ann {\n    @Id\n    @GeneratedValue\n    private Long id;\n\n    @Override\n    @Deprecated\n    public String toString() {\n        return \"\";\n    }\n}\n",
    );
    for row in [
        "\n3\t@Entity",
        "\n5-15\tpublic class Ann {",
        "\n6\t    @Id",
        "\n8\t    private Long id;",
        "\n10\t    @Override",
        "\n12-14\t    public String toString() {",
    ] {
        assert!(java.contains(row), "{row:?} in {java}");
    }
    let ts = outline(
        "dec.ts",
        "@Injectable()\n@Component({\n  selector: 'x',\n})\nexport class A {\n  @Output()\n  @HostBinding('class.x')\n  changed = 1;\n  method(): void {\n    call();\n  }\n}\n",
    );
    for row in [
        "1\t@Injectable()",
        "\n5-12\texport class A {",
        "\n6\t  @Output()",
        "\n8\t  changed = 1;",
        "\n9-11\t  method(): void {",
    ] {
        assert!(ts.contains(row), "{row:?} in {ts}");
    }
    // Every row shows once: no line both inside a span's head and its own row.
    for text in [&java, &ts] {
        let mut lines: Vec<&str> = text
            .lines()
            .filter_map(|row| row.split_once('\t')?.0.split('-').next())
            .collect();
        let all = lines.len();
        lines.dedup();
        assert_eq!(lines.len(), all, "{text}");
    }
}
