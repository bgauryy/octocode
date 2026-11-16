//! Outlines no parse size or nesting depth refuses.
use super::*;
use octocode_contract_types::ReadProgressUnit;

/// No parse size refuses an outline: code as large as a whole read admits
/// is redacted and outlined like any other, and a document format still
/// gets its keys.
#[test]
fn code_as_large_as_a_whole_read_still_outlines() {
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
        fn limits(&self) -> crate::security::SourceLimits {
            crate::security::SourceLimits::SMALL
        }
    }
    let limit = crate::security::SourceLimits::SMALL.whole_source;
    let t = Fixture::new();
    let outline = |name: &str, text: &str| {
        assert!(text.len() <= limit, "{name} is read whole");
        let scan = Counting(AtomicUsize::new(0));
        let p = t.0.join(name);
        fs::write(&p, text).expect("fixture");
        let row = fetch_with(
            &qj(&p, serde_json::json!({"view":"symbols"})),
            &Paths(t.0.clone()),
            &scan,
        );
        assert_eq!(row.view, Some(ReadView::Symbols), "{:?}", row.view_fallback);
        assert!(scan.0.load(Ordering::SeqCst) > 0, "{name} is redacted");
        row.content.expect("outline")
    };
    let head = "pub fn run() {}\n// ";
    let code = format!("{head}{}\n", "x".repeat(limit - head.len() - 1));
    assert_eq!(outline("big.rs", &code).trim_end(), "1\tpub fn run() {}");

    let (open, close) = ("{\n\"name\": 1,\n\"long\": [\n", "\"x\"\n]\n}\n");
    let rows = "\"x\",\n".repeat((limit - open.len() - close.len()) / 5);
    let data = format!("{open}{rows}{close}");
    assert!(outline("big.json", &data).contains("name"));
}

/// One outline: the localFetch outline renders the declarations document
/// astSearch symbols lists. A line nested past every parser's stack keeps
/// the file's real declaration, costs no more than that document, and says
/// which lane read the file.
#[test]
fn a_line_nested_past_the_parsers_stack_keeps_the_files_declarations() {
    let t = Fixture::new();
    let p = t.0.join("deep.js");
    let source = format!(
        "function marker() {{ return 1; }}\nconst x = {};\n",
        "(".repeat(50_000)
    );
    fs::write(&p, &source).expect("fixture");
    let r = fetch(
        &qj(&p, serde_json::json!({"view": "symbols"})),
        &Paths(t.0.clone()),
    );
    assert_eq!(r.view, Some(ReadView::Symbols), "{:?}", r.view_fallback);
    let content = r.content.as_deref().unwrap_or_default();
    assert!(
        content.starts_with("1\tfunction marker() { return 1; }"),
        "{content:.300}"
    );
    assert!(content.len() < 100, "{content:.300}");
    assert!(
        r.warnings
            .iter()
            .any(|warning| warning.contains("levels deep")),
        "{:?}",
        r.warnings
    );
    let listed = crate::tools::declarations::file_document(&source, "deep.js")
        .expect("declarations")
        .declarations
        .len();
    assert_eq!(content.lines().count(), listed, "{content:.300}");
}

/// A source as large as a whole read admits outlines every declaration the
/// shared declarations document lists: no parse size refuses it.
#[test]
fn a_source_as_large_as_a_whole_read_outlines_every_declaration() {
    let limit = SmallSafe.limits().whole_source;
    let mut source = String::new();
    let mut count = 0;
    loop {
        let function = format!(
            "int f{count}(int a) {{\n{}  return a;\n}}\n",
            "  a += 1;\n".repeat(8)
        );
        if source.len() + function.len() > limit {
            break;
        }
        source.push_str(&function);
        count += 1;
    }
    let t = Fixture::new();
    let p = t.0.join("big.h");
    fs::write(&p, &source).expect("fixture");
    // The outline's line pages, walked to the end, list every declaration.
    let (mut query, mut outlined, mut first) =
        (qj(&p, serde_json::json!({"view": "symbols"})), 0, None);
    loop {
        let r = fetch_with(&query, &Paths(t.0.clone()), &SmallSafe);
        assert_eq!(r.view, Some(ReadView::Symbols), "{:?}", r.view_fallback);
        let page = r.pagination.as_ref().expect("pagination");
        assert_eq!(page.unit, ReadProgressUnit::Lines, "{page:?}");
        let outline = r.content.expect("outline");
        outlined += outline.lines().count();
        first.get_or_insert(outline);
        let Some(next) = r.next.and_then(|next| next.continue_window) else {
            break;
        };
        query = next.query;
    }
    assert_eq!(outlined, count);
    let first = first.expect("a first page");
    assert!(
        first.starts_with("1-11\tint f0(int a) {\n"),
        "{:.200}",
        first
    );
}

/// A one-line source whose parse is estimated past the memory budget is not
/// parsed for its outline; the read falls back to the plain page and says
/// the outline was unavailable, and a plain file of the same kind is outlined.
#[test]
fn a_source_too_dense_to_parse_within_the_budget_is_not_parsed_for_an_outline() {
    let t = Fixture::new();
    let budget = octocode_engine::structural::parse_budget();
    let calls = budget / octocode_engine::structural::BYTES_PER_TOKEN / 5 + 1000;
    let dense = t.0.join("dense.js");
    fs::write(&dense, format!("var a=[{}];\n", "f(1),".repeat(calls))).expect("dense");
    let row = fetch_with(
        &qj(&dense, serde_json::json!({"view":"symbols"})),
        &Paths(t.0.clone()),
        &crate::security::ContentSecurity::new(),
    );
    assert_ne!(row.view, Some(ReadView::Symbols), "{:?}", row.view_fallback);
    assert_eq!(
        crate::tools::declarations::parses::of("declarations", &dense.to_string_lossy()),
        0,
        "the dense file was parsed"
    );
    let plain = t.0.join("plain.js");
    fs::write(
        &plain,
        format!("{}\n", "function run() {\n  return 1;\n}\n".repeat(40)),
    )
    .expect("plain");
    let row = fetch_with(
        &qj(&plain, serde_json::json!({"view":"symbols"})),
        &Paths(t.0.clone()),
        &crate::security::ContentSecurity::new(),
    );
    assert_eq!(row.view, Some(ReadView::Symbols), "{:?}", row.view_fallback);
}
