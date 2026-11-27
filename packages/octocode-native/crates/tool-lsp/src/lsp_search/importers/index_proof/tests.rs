use super::*;

fn spot(line: u32) -> Spot {
    Spot {
        line,
        character: 4,
        is_call: true,
        binding: None,
        in_code: Some(true),
    }
}

#[test]
fn only_c_family_servers_prove_sites_by_the_anchor_references() {
    for id in ["c", "cpp", "cuda"] {
        assert_eq!(SiteProof::of(Some(id)), SiteProof::AnchorReferences);
    }
    for id in ["typescript", "javascriptreact", "rust", "python"] {
        assert_eq!(SiteProof::of(Some(id)), SiteProof::DefinitionChain);
    }
    assert_eq!(SiteProof::of(None), SiteProof::DefinitionChain);
}

#[test]
fn a_listed_site_is_a_use_and_wins_over_an_unknown_one() {
    let verdict = file_verdict(
        "f.c".to_owned(),
        vec![
            (spot(1), SiteState::Listed),
            (spot(2), SiteState::Unknown),
            (spot(3), SiteState::Mention),
            (spot(4), SiteState::OtherSymbol),
        ],
    );
    assert_eq!(verdict.anchors.len(), 1);
    assert_eq!(verdict.anchors[0].line, 1);
    assert_eq!(verdict.elsewhere, [(4, 4)]);
}

#[test]
fn code_the_ast_cannot_see_leaves_the_file_undecided_a_comment_does_not() {
    let gated = file_verdict("g.c".to_owned(), vec![(spot(1), SiteState::Unknown)]);
    assert!(gated.anchors.is_empty() && gated.unresolved);
    let comment = file_verdict("c.c".to_owned(), vec![(spot(1), SiteState::Mention)]);
    assert!(comment.anchors.is_empty() && !comment.unresolved);
    let other = file_verdict("o.c".to_owned(), vec![(spot(1), SiteState::OtherSymbol)]);
    assert!(other.anchors.is_empty() && !other.unresolved);
}
