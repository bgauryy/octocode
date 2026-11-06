//! Outlines no parse size or nesting depth refuses.
use super::*;

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
    }
    let scan = Counting(AtomicUsize::new(0));
    let filler = "x".repeat(crate::security::SourceLimits::REAL.whole_source);
    let code = format!("pub fn run() {{}}\n// {filler}\n");
    let view = crate::tools::source_view::outline::symbols_view(
        &code,
        "big.rs",
        Path::new("/fixture/big.rs"),
        &scan,
    );
    assert_eq!(view.outline.as_deref(), Ok("1\tpub fn run() {}"));
    assert!(scan.0.load(Ordering::SeqCst) > 0);

    let rows = "\"x\",\n".repeat(crate::security::SourceLimits::REAL.whole_source / 4);
    let data = format!("{{\n\"name\": 1,\n\"long\": [\n{rows}\"x\"\n]\n}}\n");
    let view = crate::tools::source_view::outline::symbols_view(
        &data,
        "big.json",
        Path::new("/fixture/big.json"),
        &scan,
    );
    assert!(view.outline.expect("key outline").contains("name"));
    assert!(scan.0.load(Ordering::SeqCst) > 0);
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
    let limit = crate::security::SourceLimits::REAL.whole_source;
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
    let view = crate::tools::source_view::outline::symbols_view(
        &source,
        "big.h",
        Path::new("/fixture/big.h"),
        &Safe,
    );
    let outline = view.outline.expect("outline");
    assert_eq!(outline.lines().count(), count);
    assert!(
        outline.starts_with("1-11\tint f0(int a) {\n"),
        "{:.200}",
        outline
    );

    let t = Fixture::new();
    let p = t.0.join("big.h");
    fs::write(&p, &source).expect("fixture");
    let r = fetch(
        &qj(&p, serde_json::json!({"view": "symbols"})),
        &Paths(t.0.clone()),
    );
    assert_eq!(r.view, Some(ReadView::Symbols), "{:?}", r.view_fallback);
}
