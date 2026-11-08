use super::*;
use proptest::prelude::*;

/// `text` with `replaced` (ascending input ranges) replaced: the pass's output
/// and its edits.
fn pass(text: &str, replaced: &[(Range<usize>, &str)]) -> (String, Edits) {
    let mut out = String::new();
    let mut from = 0;
    for (range, with) in replaced {
        out.push_str(&text[from..range.start]);
        out.push_str(with);
        from = range.end;
    }
    out.push_str(&text[from..]);
    let edits = Edits::of_pass(
        replaced
            .iter()
            .map(|(range, with)| (range.clone(), with.len())),
    );
    (out, edits)
}

fn redacted(edits: &Edits) -> Vec<Range<usize>> {
    edits.redacted().collect()
}

/// A later match that straddles an earlier placeholder's edge takes the whole
/// secret that placeholder replaced with it.
#[test]
fn a_later_match_across_a_placeholder_edge_widens_to_its_whole_input() {
    let input = "a=SECRET;b";
    let (first, a) = pass(input, &[(2..8, "[R-A]")]);
    assert_eq!(first, "a=[R-A];b");
    // Pattern B matches `=[R`, the placeholder's opening edge.
    let (second, b) = pass(&first, &[(1..4, "[R-B]")]);
    assert_eq!(second, "a[R-B]-A];b");
    let both = a.then(&b);
    assert_eq!(redacted(&both), vec![1..8]);
    // The closing edge too.
    let (second, b) = pass(&first, &[(5..8, "[R-B]")]);
    assert_eq!(second, "a=[R-[R-B]b");
    assert_eq!(redacted(&a.then(&b)), vec![2..9]);
}

/// A later match that only touches an earlier placeholder still joins it.
#[test]
fn a_later_match_touching_a_placeholder_joins_it() {
    let (first, a) = pass("xSECRETyz", &[(1..7, "[R]")]);
    assert_eq!(first, "x[R]yz");
    let (_, b) = pass(&first, &[(4..5, "[S]")]);
    assert_eq!(redacted(&a.then(&b)), vec![1..8]);
}

/// Passes that never meet keep their own ranges, mapped onto the input.
#[test]
fn apart_passes_map_back_to_the_input() {
    let input = "key1=AAAA key2=BBBBBB end";
    let (first, a) = pass(input, &[(5..9, "[REDACTED-LONGER]")]);
    let b_start = first.find("BBBBBB").expect("later secret");
    let (second, b) = pass(&first, &[(b_start..b_start + 6, "[X]")]);
    assert_eq!(second, "key1=[REDACTED-LONGER] key2=[X] end");
    assert_eq!(redacted(&a.then(&b)), vec![5..9, 15..21]);
}

/// Byte ranges of multi-byte text stay byte ranges of the input.
#[test]
fn unicode_ranges_are_input_bytes() {
    let input = "é€=üüüS3crët · π=ππππ";
    let secret = input.find("üüü").expect("secret");
    let secret = secret..input.find(' ').expect("space");
    let (first, a) = pass(input, &[(secret.clone(), "[A]")]);
    let pi = first.rfind("ππππ").expect("pi");
    let (second, b) = pass(&first, &[(pi..first.len(), "[B]")]);
    assert_eq!(second, "é€=[A] · π=[B]");
    let both = a.then(&b);
    let pi_in = input.rfind("ππππ").expect("pi");
    assert_eq!(redacted(&both), vec![secret.clone(), pi_in..input.len()]);
    for (at, c) in input.char_indices() {
        let span = at..at + c.len_utf8();
        let inside = secret.contains(&at) || at >= pi_in;
        assert_eq!(both.overlaps(&span), inside, "{c:?} at {at}");
    }
}

/// A span overlaps a redaction when any of its bytes is one; a span beside it
/// does not; one that straddles its edge does.
#[test]
fn overlap_is_by_position() {
    let edits = Edits::of_pass([(4..10, 3), (20..22, 3)]);
    assert!(edits.overlaps(&(5..6)));
    assert!(edits.overlaps(&(9..12)));
    assert!(edits.overlaps(&(0..5)));
    assert!(edits.overlaps(&(0..30)));
    assert!(!edits.overlaps(&(0..4)));
    assert!(!edits.overlaps(&(10..20)));
    assert!(!edits.overlaps(&(22..30)));
    assert!(!Edits::default().overlaps(&(0..30)));
}

/// A text modeled as where each byte came from: `Some(i)` input byte `i`,
/// `None` a placeholder byte.
type Model = Vec<Option<usize>>;

/// One pass over `model`: replace each of `cuts` (as (start, len, with) in
/// ascending, disjoint order after clamping) with `with` placeholder bytes.
fn model_pass(model: &Model, cuts: &[(usize, usize, usize)]) -> (Model, Edits) {
    let mut replaced = Vec::new();
    let mut at = 0;
    for &(gap, len, with) in cuts {
        let start = at + gap;
        let end = start + len.max(1);
        if end > model.len() {
            break;
        }
        replaced.push((start..end, with));
        at = end;
    }
    let mut out = Vec::new();
    let mut from = 0;
    for (range, with) in &replaced {
        out.extend_from_slice(&model[from..range.start]);
        out.extend(std::iter::repeat_n(None, *with));
        from = range.end;
    }
    out.extend_from_slice(&model[from..]);
    (out, Edits::of_pass(replaced))
}

proptest! {
    /// Composed over any chain of passes, the edits name exactly the input
    /// bytes no longer in the output, and map every kept byte to its place.
    #[test]
    fn composed_edits_match_the_chain(
        len in 0usize..60,
        passes in proptest::collection::vec(
            proptest::collection::vec((0usize..6, 0usize..6, 0usize..5), 0..6),
            1..5,
        ),
    ) {
        let mut model: Model = (0..len).map(Some).collect();
        let mut edits = Edits::default();
        for cuts in &passes {
            let (next, pass) = model_pass(&model, cuts);
            edits = edits.then(&pass);
            model = next;
        }
        let kept: std::collections::BTreeSet<usize> = model.iter().flatten().copied().collect();
        let named: std::collections::BTreeSet<usize> = edits.redacted().flatten().collect();
        let gone: std::collections::BTreeSet<usize> = (0..len).filter(|i| !kept.contains(i)).collect();
        prop_assert_eq!(&named, &gone);
        let mut input_at = 0;
        let mut output_at = 0;
        for edit in &edits.0 {
            prop_assert!(edit.input.start >= input_at && edit.output.start >= output_at);
            for (i, o) in (input_at..edit.input.start).zip(output_at..edit.output.start) {
                prop_assert_eq!(model[o], Some(i));
            }
            prop_assert_eq!(edit.input.start - input_at, edit.output.start - output_at);
            prop_assert!(model[edit.output.clone()].iter().all(Option::is_none));
            input_at = edit.input.end;
            output_at = edit.output.end;
        }
        prop_assert_eq!(len - input_at, model.len() - output_at);
        for (i, o) in (input_at..len).zip(output_at..model.len()) {
            prop_assert_eq!(model[o], Some(i));
        }
    }
}

/// Positions map across composed passes both ways: a byte outside every
/// replacement keeps its text, a byte a replacement took has no output place,
/// and a range touching a replacement takes it whole.
#[test]
fn positions_map_between_the_input_and_the_output() {
    let input = "a=SECRET1;b=c;d=SECRET22;e";
    let (first, one) = pass(input, &[(2..9, "[R]")]);
    let (output, two) = pass(&first, &[(12..20, "[RR]")]);
    assert_eq!(output, "a=[R];b=c;d=[RR];e");
    let edits = one.then(&two);
    assert!(edits.maps(input, &output), "{output}");
    for (at, byte) in input.bytes().enumerate() {
        match edits.output_at(at) {
            Some(out) => assert_eq!(output.as_bytes()[out], byte, "{at}"),
            None => assert!(edits.overlaps(&(at..at + 1)), "{at}"),
        }
    }
    for (at, byte) in output.bytes().enumerate() {
        let back = edits.input_at(at);
        if edits.output_at(back) == Some(at) {
            assert_eq!(input.as_bytes()[back], byte, "{at}");
        } else {
            assert!(edits.overlaps(&(back..back + 1)), "{at}");
        }
    }
    assert_eq!(edits.input_range(&(0..4)), 0..9);
    assert_eq!(edits.input_range(&(5..8)), 9..12);
    assert_eq!(edits.output_end(9), 5);
    assert_eq!(edits.output_end(13), 9);
    assert_eq!(edits.output_end(20), 16);
    assert_eq!(edits.output_end(input.len()), output.len());
}
