use super::*;

#[test]
fn section_pages_preserve_source_input_arrays() {
    let source = json!({"tool":"localFetch","query":{"path":"batch.rs",
        "ranges":[{"startLine":2,"endLine":3},{"startLine":7,"endLine":8}]}});
    let sections = [2, 3, 7, 8]
        .into_iter()
        .map(|line| {
            json!({
                "id":format!("s{line}"),"kind":"function","name":format!("batch_{line}"),
                "target":{"bytes":[line*42,(line+1)*42],"lines":{"startLine":line,"endLine":line}},
                "context":[],"answers":{"evidence":{"status":"answered","answer":{"noul":0.9}}}
            })
        })
        .collect::<Vec<_>>();
    let row = json!({"index":2,"data":{"operation":"sections","resources":{"picked":{
        "status":"ready","source":source,"snapshot":"fixed","coordinateSpace":"source",
        "sections":sections
    }}}});
    let plan = PreparedRows::new(vec![row], &[None], Some(crate::id::ToolId::Clasify)).plan(700);
    assert!(plan.total_pages > 1);
    let mut seen = Vec::new();
    for page in 0..plan.total_pages {
        for row in plan.build_page(page, &[None], &[]) {
            let resource = &row["data"]["resources"]["picked"];
            assert_eq!(
                resource["source"], source,
                "source input changed on page {page}"
            );
            seen.extend(
                resource["sections"]
                    .as_array()
                    .expect("sections")
                    .iter()
                    .cloned(),
            );
        }
    }
    assert_eq!(seen, sections);
}
