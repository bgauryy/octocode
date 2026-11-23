//! The SCIP reader, symbol mapping and the call linker's precise tier, over
//! hand-encoded SCIP bytes (never a real indexer's output).
use super::*;
use crate::ast_graph::graph::{GraphScan, build_graph};
use crate::ast_graph::store::format::Confidence;
use crate::ast_graph::store::format::EdgeKind;
use crate::ast_graph::store::tables::{Projection, project_with_scip};
use crate::ast_graph::store::workspace::Workspace;
use crate::ast_graph::test_support::{CARGO_APP, fixture};
use octocode_runtime_base::execution::cancel::NeverCancel;

// ── A tiny SCIP encoder ──────────────────────────────────────────────────────

fn varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value & 0x7f) as u8 | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn field(out: &mut Vec<u8>, number: u64, payload: &[u8]) {
    varint(out, number << 3 | 2);
    varint(out, payload.len() as u64);
    out.extend_from_slice(payload);
}

fn number(out: &mut Vec<u8>, field_number: u64, value: u64) {
    varint(out, field_number << 3);
    varint(out, value);
}

/// One occurrence over `range` (3 or 4 numbers), with an unknown field
/// (`enclosing_range`, 7) the reader must skip.
pub(crate) fn occurrence(range: &[u64], symbol: &str, roles: u64) -> Vec<u8> {
    let mut packed = Vec::new();
    range.iter().for_each(|n| varint(&mut packed, *n));
    let mut out = Vec::new();
    field(&mut out, 1, &packed);
    field(&mut out, 2, symbol.as_bytes());
    if roles != 0 {
        number(&mut out, 3, roles);
    }
    field(&mut out, 7, &packed);
    out
}

/// A document of `path`, its encoding (`PositionEncoding`) and occurrences.
pub(crate) fn document(path: &str, encoding: u64, occurrences: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    field(&mut out, 1, path.as_bytes());
    occurrences.iter().for_each(|o| field(&mut out, 2, o));
    field(&mut out, 4, b"rust");
    number(&mut out, 6, encoding);
    out
}

pub(crate) fn index(documents: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    field(&mut out, 1, b"metadata");
    documents.iter().for_each(|d| field(&mut out, 2, d));
    field(&mut out, 3, b"external symbols");
    out
}

/// Zero-based `(line, column)` of the `nth` `needle` in `text`, with the
/// column counted in UTF-8 bytes, UTF-16 units and chars.
pub(crate) fn at(text: &str, nth: usize, needle: &str) -> (u64, [u64; 3]) {
    let offset = text.match_indices(needle).nth(nth).expect("needle").0;
    let line = text[..offset].matches('\n').count();
    let row = text[..offset].rsplit('\n').next().unwrap_or("");
    (
        line as u64,
        [
            row.len() as u64,
            row.chars().map(char::len_utf16).sum::<usize>() as u64,
            row.chars().count() as u64,
        ],
    )
}

pub(crate) const UTF8: u64 = 1;
const UTF16: u64 = 2;
const UTF32: u64 = 3;

/// The occurrence of the `nth` `needle`; `encoding` picks the column kind.
pub(crate) fn occ(
    text: &str,
    nth: usize,
    needle: &str,
    symbol: &str,
    roles: u64,
    encoding: u64,
) -> Vec<u8> {
    let (line, cols) = at(text, nth, needle);
    let col = cols[(encoding.max(1) - 1) as usize];
    let width = match encoding {
        UTF8 => needle.len(),
        UTF16 => needle.encode_utf16().count(),
        _ => needle.chars().count(),
    } as u64;
    occurrence(&[line, col, col + width], symbol, roles)
}

pub(crate) fn sym(name: &str) -> String {
    format!("rust-analyzer cargo app 0.1.0 {name}().")
}

// ── Reader ───────────────────────────────────────────────────────────────────

#[test]
fn reader_round_trips_documents_occurrences_and_skips_the_rest() {
    let text = "fn a() {}\nfn b() { a(); }\n";
    let bytes = index(&[
        document(
            "src/lib.rs",
            UTF8,
            &[
                occ(text, 0, "a", &sym("a"), 1, UTF8),
                occ(text, 1, "a", &sym("a"), 0, UTF8),
                occ(text, 0, "b", "local 7", 1, UTF8),
                occurrence(&[1, 3, 1, 4], &sym("b"), 1),
            ],
        ),
        document("src/other.rs", UTF16, &[]),
    ]);
    let docs = reader::documents(&bytes).unwrap();
    assert_eq!(docs.len(), 2);
    assert_eq!(docs[0].path, "src/lib.rs");
    assert_eq!(docs[0].encoding, PositionEncoding::Utf8);
    assert_eq!(docs[1].encoding, PositionEncoding::Utf16);
    let first = docs[0].occurrences[0];
    assert_eq!(
        (
            first.start_line,
            first.start_col,
            first.end_line,
            first.end_col,
            first.roles
        ),
        (0, 3, 0, 4, 1)
    );
    assert_eq!(first.symbol, sym("a"));
    // The four-number form carries its own end line.
    assert_eq!(docs[0].occurrences[3].end_line, 1);

    let parsed = ScipIndex::parse(&bytes).unwrap();
    assert!(parsed.has_document("src/lib.rs") && !parsed.has_document("nope.rs"));
    // `local ` symbols never enter the symbol table: `a` and `b` only.
    assert_eq!(parsed.symbols.len(), 2);
}

#[test]
fn malformed_bytes_are_typed_errors_and_never_panic() {
    let text = "fn a() {}\n";
    let good = index(&[document(
        "src/lib.rs",
        UTF8,
        &[occ(text, 0, "a", &sym("a"), 1, UTF8)],
    )]);
    assert!(ScipIndex::parse(&good).is_ok());
    // A group wire type, a length past the end, a varint without end, a
    // range of two numbers, and a symbol that is not UTF-8.
    let mut group = Vec::new();
    varint(&mut group, 9 << 3 | 3);
    let mut long = Vec::new();
    varint(&mut long, 2 << 3 | 2);
    varint(&mut long, 1000);
    let endless = vec![
        0x10, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    ];
    let short_range = {
        let mut o = Vec::new();
        field(&mut o, 1, &[0, 1]);
        index(&[document("a.rs", UTF8, &[o])])
    };
    let bad_text = {
        let mut o = Vec::new();
        field(&mut o, 2, &[0xff, 0xfe]);
        index(&[document("a.rs", UTF8, &[o])])
    };
    for (bytes, expected) in [
        (group, "wire"),
        (long, "length"),
        (endless, "varint"),
        (short_range, "range"),
        (bad_text, "UTF-8"),
    ] {
        let error = ScipIndex::parse(&bytes).err().expect("error").to_string();
        assert!(error.contains(expected), "{error}");
    }
    // Every prefix of a valid index parses or errors; none panics.
    for end in 0..good.len() {
        let _ = ScipIndex::parse(&good[..end]);
    }
}

#[test]
fn utf16_and_utf32_columns_become_utf8_bytes() {
    // `é` is two bytes, `🎉` four bytes, two UTF-16 units, one scalar.
    let text = "let s = \"é🎉\"; alpha();\n";
    let (_, cols) = at(text, 0, "alpha");
    assert!(cols[0] > cols[1] && cols[1] > cols[2], "{cols:?}");
    for encoding in [UTF8, UTF16, UTF32] {
        let bytes = index(&[document(
            "a.rs",
            encoding,
            &[occ(text, 0, "alpha", &sym("alpha"), 0, encoding)],
        )]);
        let mut parsed = ScipIndex::parse(&bytes).unwrap();
        assert!(parsed.bind_file("a.rs", text, &[]));
        let want = cols[0] as u32;
        let found = parsed
            .target_at("a.rs", 0, want, want + 5)
            .expect("covered");
        assert_eq!(found.symbol(), sym("alpha"), "encoding {encoding}");
        assert!(
            parsed
                .target_at("a.rs", 0, cols[2] as u32, cols[2] as u32 + 5)
                .is_none()
        );
    }
}

// ── Symbol to declaration mapping ────────────────────────────────────────────

fn decl(node: u32, name: &str, line: u32, selection: Option<(u32, u32)>) -> ScipDecl<'_> {
    ScipDecl {
        node,
        name,
        line,
        selection,
    }
}

#[test]
fn a_definition_maps_by_selection_then_by_line_and_name_else_not_at_all() {
    let text = "fn alpha() {}\nfn beta() {}\nfn beta2() {}\n";
    let bytes = index(&[document(
        "a.rs",
        UTF8,
        &[
            occ(text, 0, "alpha", &sym("alpha"), 1, UTF8),
            occ(text, 0, "beta", &sym("beta"), 1, UTF8),
            occ(text, 0, "beta2", &sym("beta2"), 1, UTF8),
        ],
    )]);
    let mut parsed = ScipIndex::parse(&bytes).unwrap();
    // `alpha`: by selection start (byte 3). `beta`: no selection, one
    // declaration of that name on its line. `beta2`: two on its line.
    parsed.bind_file(
        "a.rs",
        text,
        &[
            decl(1, "alpha", 9, Some((3, 8))),
            decl(2, "beta", 2, None),
            decl(3, "beta2", 3, None),
            decl(4, "beta2", 3, None),
        ],
    );
    let found = |name: &str| {
        parsed
            .symbols
            .iter()
            .find(|s| s.symbol() == sym(name))
            .and_then(ScipSymbol::declaration)
    };
    assert_eq!(found("alpha"), Some(1));
    assert_eq!(found("beta"), Some(2));
    assert_eq!(found("beta2"), None);
}

// ── Linker override ──────────────────────────────────────────────────────────

const LIB: &str = "pub fn alpha() {}\npub fn beta() {}\npub fn run() {\n    alpha();\n}\n";

/// Calls edges as `(caller, callee, detail)`, sorted, and the projection.
fn linked(
    source: &str,
    scip: Option<&mut ScipIndex>,
    plain: bool,
) -> (Vec<(String, String, String)>, Projection) {
    let temp = fixture(&[CARGO_APP, ("src/lib.rs", source)]);
    let root = temp.0.as_path();
    let paths = octocode_runtime_base::test_support::workspace_policy(root);
    let mut built = build_graph(
        &GraphScan {
            root: root.to_path_buf(),
            ..Default::default()
        },
        &paths,
        &octocode_runtime_base::security::ContentSecurity::new(),
        &NeverCancel,
    )
    .unwrap();
    let workspace = Workspace::default();
    let text = |file: &str| (file == "src/lib.rs").then(|| source.to_owned());
    let input = scip
        .filter(|_| !plain)
        .map(|index| ScipInput { index, text: &text });
    let projection = project_with_scip(&mut built, &workspace, &NeverCancel, input).unwrap();
    let tables = &projection.tables;
    let string = |id: u32| tables.strings.get(id as usize).unwrap().to_owned();
    let mut rows = tables
        .edges
        .iter()
        .filter(|edge| edge.kind == EdgeKind::Calls)
        .map(|edge| {
            (
                string(tables.nodes[edge.src as usize].key),
                string(tables.nodes[edge.dst as usize].key),
                string(edge.detail),
            )
        })
        .collect::<Vec<_>>();
    rows.sort();
    (rows, projection)
}

fn parsed(occurrences: &[Vec<u8>]) -> ScipIndex {
    ScipIndex::parse(&index(&[document("src/lib.rs", UTF8, occurrences)])).unwrap()
}

/// Definition occurrences of `alpha` and `beta`, over their name tokens.
fn defs(text: &str) -> Vec<Vec<u8>> {
    ["alpha", "beta"]
        .iter()
        .map(|name| occurrence(&at_def(text, name), &sym(name), 1))
        .collect()
}

#[test]
fn scip_confirms_a_syntax_edge_that_agrees() {
    let mut occurrences = defs(LIB);
    occurrences.push(occ(LIB, 1, "alpha", &sym("alpha"), 0, UTF8));
    let mut index = parsed(&occurrences);
    let (before, _) = linked(LIB, None, true);
    assert_eq!(
        before,
        [(
            "src/lib.rs#run".into(),
            "src/lib.rs#alpha".into(),
            "local".into()
        )]
    );
    let (rows, projection) = linked(LIB, Some(&mut index), false);
    assert_eq!(
        rows,
        [(
            "src/lib.rs#run".into(),
            "src/lib.rs#alpha".into(),
            "scip".into()
        )]
    );
    assert_eq!(projection.calls.by_resolution.get("scip"), Some(&1));
    let edge = projection
        .tables
        .edges
        .iter()
        .find(|e| e.kind == EdgeKind::Calls)
        .unwrap();
    assert_eq!(edge.confidence, Confidence::High);
}

#[test]
fn scip_replaces_a_syntax_edge_that_disagrees() {
    let mut occurrences = defs(LIB);
    occurrences.push(occ(LIB, 1, "alpha", &sym("beta"), 0, UTF8));
    let mut index = parsed(&occurrences);
    let (rows, _) = linked(LIB, Some(&mut index), false);
    assert_eq!(
        rows,
        [(
            "src/lib.rs#run".into(),
            "src/lib.rs#beta".into(),
            "scip".into()
        )]
    );
}

const AMBIGUOUS: &str = "pub struct A;\npub struct B;\nimpl A { pub fn hidden(&self) {} }\nimpl B { pub fn hidden(&self) {} }\npub fn go(x: Unknown) {\n    x.hidden();\n}\n";

#[test]
fn scip_resolves_a_site_the_syntax_could_not() {
    let (before, plain) = linked(AMBIGUOUS, None, true);
    assert!(
        before.iter().all(|(caller, ..)| caller != "src/lib.rs#go"),
        "{before:?}"
    );
    assert!(plain.calls.unresolved >= 1);
    let (line, col) = at(AMBIGUOUS, 1, "fn hidden");
    let definition = occurrence(&[line, col[0] + 3, col[0] + 9], &sym("B#hidden"), 1);
    let first = at(AMBIGUOUS, 0, "fn hidden");
    let other = occurrence(
        &[first.0, first.1[0] + 3, first.1[0] + 9],
        &sym("A#hidden"),
        1,
    );
    let mut index = parsed(&[
        other,
        definition,
        occ(AMBIGUOUS, 2, "hidden", &sym("B#hidden"), 0, UTF8),
    ]);
    let (rows, projection) = linked(AMBIGUOUS, Some(&mut index), false);
    assert!(
        rows.contains(&(
            "src/lib.rs#go".into(),
            "src/lib.rs#B.hidden".into(),
            "scip".into()
        )),
        "{rows:?}"
    );
    assert!(projection.calls.unresolved < plain.calls.unresolved);
}

#[test]
fn an_ambiguous_site_keeps_the_syntax_result() {
    // Two symbols at the call's name token: no answer.
    let mut occurrences = defs(LIB);
    occurrences.push(occ(LIB, 1, "alpha", &sym("alpha"), 0, UTF8));
    occurrences.push(occ(LIB, 1, "alpha", &sym("beta"), 0, UTF8));
    let mut two = parsed(&occurrences);
    let (kept, _) = linked(LIB, Some(&mut two), false);
    let (plain, _) = linked(LIB, None, true);
    assert_eq!(kept, plain);
    // A symbol with no definition in the graph names no declaration.
    let mut external = parsed(&[occ(LIB, 1, "alpha", &sym("elsewhere"), 0, UTF8)]);
    assert_eq!(linked(LIB, Some(&mut external), false).0, plain);
    // A reference whose symbol the definitions leave ambiguous.
    let mut occurrences = defs(LIB);
    occurrences[0] = occurrence(&at_def(LIB, "alpha"), &sym("beta"), 1);
    occurrences.push(occ(LIB, 1, "alpha", &sym("beta"), 0, UTF8));
    let mut split = parsed(&occurrences);
    assert_eq!(linked(LIB, Some(&mut split), false).0, plain);
}

fn at_def(text: &str, name: &str) -> [u64; 3] {
    let (line, cols) = at(text, 0, &format!("fn {name}"));
    [line, cols[0] + 3, cols[0] + 3 + name.len() as u64]
}

#[test]
fn no_index_or_an_index_without_the_file_changes_nothing() {
    let (plain, plain_projection) = linked(LIB, None, true);
    let (none, none_projection) = linked(LIB, None, false);
    let mut other = ScipIndex::parse(&index(&[document("src/other.rs", UTF8, &[])])).unwrap();
    let (unrelated, unrelated_projection) = linked(LIB, Some(&mut other), false);
    assert_eq!(plain, none);
    assert_eq!(plain, unrelated);
    for projection in [&none_projection, &unrelated_projection] {
        assert_eq!(
            projection.calls.by_resolution,
            plain_projection.calls.by_resolution
        );
        assert_eq!(projection.calls.by_resolution.get("scip"), None);
    }
}

#[test]
fn a_non_ascii_line_still_lands_on_the_call_token() {
    // The site sits after a two-unit emoji in a UTF-16 document: a column
    // misread as bytes would miss the token and keep the syntax edge.
    let source =
        "pub fn alpha() {}\npub fn beta() {}\npub fn run() {\n    let _s = \"é🎉\"; alpha();\n}\n";
    let mut occurrences = defs(source);
    occurrences.push(occ(source, 1, "alpha", &sym("beta"), 0, UTF16));
    let bytes = index(&[document("src/lib.rs", UTF16, &occurrences)]);
    let mut index = ScipIndex::parse(&bytes).unwrap();
    let (rows, _) = linked(source, Some(&mut index), false);
    assert_eq!(
        rows,
        [(
            "src/lib.rs#run".into(),
            "src/lib.rs#beta".into(),
            "scip".into()
        )]
    );
}
