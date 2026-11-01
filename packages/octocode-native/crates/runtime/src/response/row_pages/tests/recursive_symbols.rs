use super::*;
use crate::id::ToolId;
use crate::tools::test_support::assert_row_contract;
use tokio_util::sync::CancellationToken;

fn collect_symbols(symbols: &Value, aliases: &mut Vec<String>, leaves: &mut Vec<Value>) {
    for symbol in symbols.as_array().expect("symbols") {
        assert_eq!(symbol["kind"], "class");
        assert_eq!(symbol["exported"], true);
        assert_eq!(symbol["sourceRange"], json!({"startByte":0,"endByte":100}));
        assert_eq!(
            symbol["selectionByteRange"],
            json!({"startByte":2,"endByte":4})
        );
        aliases.extend(
            symbol["exportedAs"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|alias| alias.as_str().expect("alias").to_owned()),
        );
        if symbol["symbolName"] == "Leaf" {
            leaves.push(symbol.clone());
        }
        if let Some(members) = symbol.get("members") {
            collect_symbols(members, aliases, leaves);
        }
    }
}

#[test]
fn deeply_nested_symbols_page_every_alias_and_preserve_required_identity() {
    let aliases = (0..80)
        .map(|index| format!("alias-{index}-{}", "public-name".repeat(5)))
        .collect::<Vec<_>>();
    let leaf = json!({"symbolName":"Leaf","kind":"class","line":10,"exported":true,
        "sourceRange":{"startByte":0,"endByte":100},"selectionByteRange":{"startByte":2,"endByte":4}});
    let mut symbol = leaf.clone();
    for depth in (0..8).rev() {
        symbol = json!({"symbolName":format!("Container{depth}"),"kind":"class","line":depth+1,
            "exported":true,"exportedAs":if depth == 7 {aliases.clone()}else{Vec::new()},
            "members":[symbol],"sourceRange":{"startByte":0,"endByte":100},
            "selectionByteRange":{"startByte":2,"endByte":4}});
    }
    let data = json!({"symbols":[symbol]});
    assert_row_contract(ToolId::AstSearch, &data);
    let plan = PreparedRows::new(
        vec![json!({"index":0,"data":data})],
        &[None],
        ToolId::AstSearch,
    )
    .plan(700, &CancellationToken::new())
    .expect("valid plan");
    assert!(
        plan.total_pages > 2,
        "deep aliases must be independently pageable"
    );
    let (mut seen_aliases, mut seen_leaves) = (Vec::new(), Vec::new());
    for page in 0..plan.total_pages {
        for row in plan
            .build_page(page, &[None], &[], &CancellationToken::new())
            .expect("valid build_page")
        {
            assert_row_contract(ToolId::AstSearch, &row["data"]);
            collect_symbols(&row["data"]["symbols"], &mut seen_aliases, &mut seen_leaves);
        }
    }
    assert_eq!(seen_aliases, aliases);
    assert_eq!(seen_leaves, vec![leaf]);
}
