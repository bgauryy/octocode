use super::{Token, refines};

fn tokens<'a>(text: &'a str, cuts: &[(usize, usize)]) -> Vec<Token<'a>> {
    cuts.iter()
        .map(|&(start, end)| Token {
            text: &text[start..end],
            aggregate: false,
        })
        .collect()
}

#[test]
fn preprocessor_aggregation_never_hides_changed_literal_bytes() {
    let source = "#define Message \"a  b\" \r";
    for view in ["#define Message \"a  b\"", "#define Message \"a b\""] {
        let before = [Token {
            text: source,
            aggregate: true,
        }];
        let after = [Token {
            text: view,
            aggregate: true,
        }];
        assert_eq!(
            refines(source, &before, view, &after).is_ok(),
            view.contains("a  b")
        );
    }
}

#[test]
fn a_split_of_the_same_bytes_passes_and_a_glue_fails() {
    // `>>` kept as `>>`, read as one token before and two after.
    let (source, view) = ("a>>b", "a>>b");
    let before = tokens(source, &[(0, 1), (1, 3), (3, 4)]);
    let after = tokens(view, &[(0, 1), (1, 2), (2, 3), (3, 4)]);
    assert!(refines(source, &before, view, &after).is_ok());
    // `> >` emitted as `>>`: two tokens glued into one.
    let (source, view) = ("a> >b", "a>>b");
    let before = tokens(source, &[(0, 1), (1, 2), (3, 4), (4, 5)]);
    let after = tokens(view, &[(0, 1), (1, 3), (3, 4)]);
    assert!(refines(source, &before, view, &after).is_err());
    // A split whose bytes changed (`> >` read as one token, emitted `>>`
    // read as two) is a change too.
    let (source, view) = ("a> >b", "a>>b");
    let before = tokens(source, &[(0, 1), (1, 4), (4, 5)]);
    let after = tokens(view, &[(0, 1), (1, 2), (2, 3), (3, 4)]);
    assert!(refines(source, &before, view, &after).is_err());
}
