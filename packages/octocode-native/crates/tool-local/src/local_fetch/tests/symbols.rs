//! The symbols view: outlines, redactions, gutters and spans.

use super::*;
use octocode_contract_types::ReadView;
use octocode_test_support::json_fixture;

mod parse_limits;

/// A source whose earlier redaction (a whole-file key block) broke the
/// parse names those lines as redacted text, never as syntax the grammar
/// lacks.
#[test]
fn a_symbols_outline_names_lines_a_redaction_left_unparseable() {
    let t = Fixture::new();
    let p = t.0.join("k.rs");
    fs::write(
        &p,
        "pub fn a() -> u8 {\n    let key = 1;\n[REDACTED-PRIVATEKEYFRAGMENT]\n    key\n}\n\npub fn b() {}\n",
    )
    .expect("fixture");
    let row = fetch_with(
        &qj(&p, json_fixture(r#"{"view": "symbols"}"#)),
        &Paths(t.0.clone()),
        &octocode_runtime_base::security::ContentSecurity::new(),
    );
    assert_eq!(row.view, Some(ReadView::Symbols), "{:?}", row.view_fallback);
    assert!(
        row.warnings
            .iter()
            .any(|warning| warning.contains("secret-shaped text was redacted")),
        "{:?}",
        row.warnings
    );
    assert!(
        !row.warnings
            .iter()
            .any(|warning| warning.contains("grammar lacks")),
        "{:?}",
        row.warnings
    );
}

/// The symbols outline contains only redacted source and names each changed line.
#[test]
fn a_symbols_view_never_holds_a_secret_of_the_source() {
    let token = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
    let t = Fixture::new();
    let p = t.0.join("sec.rs");
    fs::write(
        &p,
        format!("pub const GH: &str = \"{token}\";\n\npub fn run() -> u8 {{\n    1\n}}\n"),
    )
    .expect("fixture");
    let row = fetch_with(
        &qj(&p, json_fixture(r#"{"view": "symbols"}"#)),
        &Paths(t.0.clone()),
        &octocode_runtime_base::security::ContentSecurity::new(),
    );
    assert_eq!(row.view, Some(ReadView::Symbols), "{:?}", row.view_fallback);
    let outline = row.content.expect("outline");
    assert!(outline.contains("[REDACTED"), "{outline}");
    assert!(!outline.contains(token), "{outline}");
    assert!(
        row.warnings
            .iter()
            .any(|warning| warning == octocode_runtime_base::security::REDACTED_MARK),
        "{:?}",
        row.warnings
    );
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
    let row = fetch(&qj(&p, json_fixture(r#"{"view": "symbols"}"#)), &paths);
    let content = row.content.expect("content");
    assert!(!content.contains("SECRET"), "{content}");
    assert!(content.contains("[REDACTED]"), "{content}");
    assert!(
        row.warnings
            .iter()
            .any(|warning| warning == octocode_runtime_base::security::REDACTED_MARK),
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
    let content = fetch(&qj(&p, json_fixture(r#"{"view": "symbols"}"#)), &paths)
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
    let content = fetch(&qj(&p, json_fixture(r#"{"view": "symbols"}"#)), &paths)
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
    let r = fetch(&qj(&json, json_fixture(r#"{"view": "symbols"}"#)), &paths);
    assert_eq!(r.view, Some(ReadView::Symbols), "{r:?}");
    assert_eq!(
        r.content.as_deref(),
        Some("2\t  \"name\": \"x\",\n3-205\t  \"items\": {\n")
    );
    let text = t.0.join("notes.txt");
    fs::write(&text, "plain\n".repeat(100)).expect("fixture");
    let r = fetch(&qj(&text, json_fixture(r#"{"view": "symbols"}"#)), &paths);
    // No outline, and a compact view would only lose whitespace (no grammar
    // reads `.txt`): the row says both reasons and leads to the lines,
    // instead of near-raw text labelled compact.
    assert_eq!(r.error_code, Some(ErrorCode::SymbolsUnsupported), "{r:?}");
    assert_eq!(r.content, None, "{r:?}");
    assert!(
        r.error
            .as_deref()
            .is_some_and(|error| error.contains("unsupported-language; whitespace-only-no-grammar")),
        "{r:?}"
    );
    let read = r.next.as_ref().and_then(|next| next.read.as_ref());
    assert!(read.is_some(), "{r:?}");
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
        &qj(&p, json_fixture(r#"{"view": "symbols"}"#)),
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
            json_fixture(
                r#"{
                    "view": "symbols",
                    "readProgress": {"unit": "lines", "offset": 5}
                }"#,
            ),
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

/// Each symbols request sanitizes its own source; no prior answer is retained.
#[test]
fn each_symbols_view_scans_its_own_source() {
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
    let scan = Counting(AtomicUsize::new(0));
    let t = Fixture::new();
    let p = t.0.join("run.rs");
    fs::write(&p, "pub fn run() -> u8 {\n    1\n}\n").expect("fixture");
    let query = qj(&p, json_fixture(r#"{"view": "symbols"}"#));
    let view = |scan: &Counting| fetch_with(&query, &Paths(t.0.clone()), scan);
    let first = view(&scan);
    let scans = scan.0.load(Ordering::SeqCst);
    assert!(scans > 0);
    let again = view(&scan);
    assert_eq!(first, again);
    assert_eq!(scan.0.load(Ordering::SeqCst), scans * 2);
}

/// A symbols outline names each multi-line declaration's line span.
#[test]
fn symbols_view_heads_carry_line_ranges() {
    let t = Fixture::new();
    let p = t.0.join("m.py");
    fs::write(&p, PY_TWO).expect("fixture");
    let content = fetch(
        &qj(&p, json_fixture(r#"{"view": "symbols"}"#)),
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
        fetch(&qj(&p, json_fixture(r#"{"view": "symbols"}"#)), &paths)
            .content
            .expect("content")
    };
    let java = outline(
        "Ann.java",
        "package x;\n\n@Entity\n@Table(name = \"t\")\npublic class Ann {\n    @Id\n    @GeneratedValue\n    private Long id;\n\n    @Override\n    @Deprecated\n    public String toString() {\n        return \"\";\n    }\n}\n",
    );
    // An annotation run opening a declaration keeps its first row, marked
    // cut when the run goes on.
    let has_row = |text: &str, row: &str| text.lines().any(|line| line.starts_with(row));
    for row in [
        "3\t@Entity",
        "5-15\tpublic class Ann {",
        "6\t    @Id",
        "8\t    private Long id;",
        "10\t    @Override",
        "12-14\t    public String toString() {",
    ] {
        assert!(has_row(&java, row), "{row:?} in {java}");
    }
    let ts = outline(
        "dec.ts",
        "@Injectable()\n@Component({\n  selector: 'x',\n})\nexport class A {\n  @Output()\n  @HostBinding('class.x')\n  changed = 1;\n  method(): void {\n    call();\n  }\n}\n",
    );
    for row in [
        "1\t@Injectable()",
        "5-12\texport class A {",
        "6\t  @Output()",
        "8\t  changed = 1;",
        "9-11\t  method(): void {",
    ] {
        assert!(has_row(&ts, row), "{row:?} in {ts}");
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

/// A compact read says on its first page how many bytes its view keeps of
/// what the content view serves for the same lines, gutters included, so
/// the agent can choose it or the source: no threshold chooses for it.
#[test]
fn a_compact_read_says_how_much_of_the_source_it_keeps() {
    let t = Fixture::new();
    let p = t.0.join("c.rs");
    let source = "// a comment\nfn a() -> u8 {\n    // another\n    1\n}\n";
    fs::write(&p, source).expect("fixture");
    let r = fetch(
        &qj(&p, json_fixture(r#"{"view": "compact"}"#)),
        &Paths(t.0.clone()),
    );
    assert_eq!(r.view, Some(ReadView::Compact), "{:?}", r.view_fallback);
    let kept = r.content.as_deref().expect("compact").len();
    let content = "1\t// a comment\n2\tfn a() -> u8 {\n3\t    // another\n4\t    1\n5\t}\n";
    let note = format!(
        "compact view: {kept} of {} bytes the content view serves",
        content.len()
    );
    assert!(
        r.warnings.iter().any(|warning| warning.starts_with(&note)),
        "{note:?} in {:?}",
        r.warnings
    );
}

/// The compact note measures the whole selection, not the page that states
/// it: a paged compact read says so, and its byte counts are every page's.
#[test]
fn a_paged_compact_note_measures_the_whole_selection() {
    let t = Fixture::new();
    let p = t.0.join("p.rs");
    let source = (1..=12)
        .map(|n| format!("// note {n}\nfn f{n}() -> u8 {{ {n} }}\n"))
        .collect::<String>();
    fs::write(&p, &source).expect("fixture");
    let whole = fetch(
        &qj(&p, json_fixture(r#"{"view": "compact"}"#)),
        &Paths(t.0.clone()),
    );
    let paged = fetch(
        &qj(&p, json_fixture(r#"{"view": "compact", "limit": 3}"#)),
        &Paths(t.0.clone()),
    );
    assert!(
        paged.pagination.as_ref().is_some_and(|page| page.has_more),
        "a paged read: {:?}",
        paged.pagination
    );
    let note = |r: &LocalFetchResult| {
        r.warnings
            .iter()
            .find(|warning| warning.starts_with("compact view: "))
            .cloned()
            .unwrap_or_else(|| panic!("compact note in {:?}", r.warnings))
    };
    let kept = whole.content.as_deref().expect("compact").len();
    let content = source
        .lines()
        .enumerate()
        .map(|(index, line)| format!("{}\t{line}\n", index + 1))
        .collect::<String>()
        .len();
    assert_eq!(
        note(&paged),
        note(&whole),
        "every page's bytes, whichever page says it"
    );
    assert!(
        note(&paged).starts_with(&format!(
            "compact view: {kept} of {content} bytes the content view serves for the selected lines, all pages ("
        )),
        "{}",
        note(&paged)
    );
}

/// A compact view no smaller than the content view of the same lines (here
/// nothing to remove) is not served as compact: the content view is, with
/// `viewFallback` saying why, and the first page still says what compact
/// would keep.
#[test]
fn a_compact_view_no_smaller_than_the_content_view_serves_the_content() {
    let t = Fixture::new();
    let p = t.0.join("n.rs");
    let source = "fn a() {}\nfn b() {}\n";
    fs::write(&p, source).expect("fixture");
    let r = fetch(
        &qj(&p, json_fixture(r#"{"view": "compact"}"#)),
        &Paths(t.0.clone()),
    );
    assert_eq!(r.view, None, "{:?}", r.view_fallback);
    let fallback = r.view_fallback.as_ref().expect("viewFallback");
    assert_eq!(
        (
            fallback.requested,
            fallback.applied,
            fallback.reason.as_str()
        ),
        (ReadView::Compact, ReadView::Content, "not-smaller")
    );
    // The content view as the runtime serves it; the response numbers it.
    assert_eq!(r.content.as_deref(), Some(source));
    assert!(
        r.warnings
            .iter()
            .any(|warning| warning
                .starts_with("compact view: 24 of 24 bytes the content view serves")),
        "{:?}",
        r.warnings
    );
}
