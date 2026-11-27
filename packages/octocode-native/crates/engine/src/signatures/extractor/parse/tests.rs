use super::*;
use crate::signatures::languages;

fn grammar(extension: &str) -> &'static Language {
    &languages::find_entry(extension)
        .expect("a grammar")
        .language
}

/// An unclosed regex class repeated on one line: the lexer re-reads the line
/// from each one.
#[test]
fn recovery_that_rereads_the_source_stops_at_the_bound() {
    let source = "a=/[ ".repeat(64 * 1024 / 5);
    let (parsed, work) = run(&source, "", grammar("js"), None, &|| false);
    assert!(matches!(parsed, Err(ParseFailure::Unbounded)), "{work:?}");
}

#[test]
fn valid_code_stays_inside_the_bound() {
    let source = "fn f(a: u8) -> u8 { a + 1 }\n".repeat(4096);
    let (parsed, work) = run(&source, "", grammar("rs"), None, &|| false);
    assert!(parsed.is_ok(), "{work:?}");
}

#[test]
fn a_stop_or_a_cancelled_job_ends_the_parse() {
    assert!(matches!(
        parse_until_stopped("fn f() {}", grammar("rs"), &|| true),
        Err(ParseFailure::Interrupted)
    ));
    let cancelled = crate::deep_stack::tests::run_as_cancelled_job(|| {
        parse_until_stopped("fn f() {}", grammar("rs"), &|| false).is_err()
    });
    assert!(cancelled);
}

/// A ranged parse leaves no included ranges behind for the next parse.
#[test]
fn a_ranged_parse_leaves_the_next_parse_whole() {
    let source = "fn a() {}\nfn b() {}\n";
    let ranges = [tree_sitter::Range {
        start_byte: 0,
        end_byte: 9,
        start_point: tree_sitter::Point::new(0, 0),
        end_point: tree_sitter::Point::new(0, 9),
    }];
    assert!(parse_ranges_before(source, grammar("rs"), &ranges, whole_file_deadline()).is_some());
    let whole = parse_with_deadline(source, grammar("rs"), whole_file_deadline()).expect("tree");
    assert_eq!(whole.root_node().named_child_count(), 2);
}

/// A chunk never splits a character.
#[test]
fn a_read_chunk_ends_on_a_char_boundary() {
    let text = format!("{}é tail", "a".repeat(READ_CHUNK - 1));
    let piece = chunk(&text, 0);
    assert!(std::str::from_utf8(piece).is_ok());
    assert_eq!(piece.len(), READ_CHUNK + 1);
}
