use super::*;
use crate::id::ToolId;
use crate::tools::test_support::assert_row_contract;
use std::collections::BTreeMap;
use tokio_util::sync::CancellationToken;

#[test]
fn nested_clasify_fragments_validate_and_preserve_every_answer_and_its_context() {
    let questions = (0..7)
        .map(|question| {
            let criteria = (0..13)
                .map(|option| (format!("option-{option}"), Value::Null))
                .collect::<Map<_, _>>();
            (
                format!("q{question}"),
                json!({"type":"choice","criteria":criteria}),
            )
        })
        .collect::<Map<_, _>>();
    let mut expected = BTreeMap::new();
    let mut resources = Map::new();
    for resource in ["0", "r.other"] {
        let sections = (0..3).map(|section| {
            let answers = (0..7).map(|question| {
                let probabilities = (0..13)
                    .map(|option| (format!("option-{option}"), json!(1.0 / 13.0)))
                    .collect::<Map<_, _>>();
                let answer = json!({"status":"answered","answer":{
                    "choice":"option-0","confidence":0.0,"probabilities":probabilities
                }});
                expected.insert((resource.to_owned(), format!("s{section}"), format!("q{question}")), answer.clone());
                (format!("q{question}"), answer)
            }).collect::<Map<_, _>>();
            json!({"id":format!("s{section}"),"kind":"function","name":format!("target{section}"),
                "target":{"bytes":[section*100,(section+1)*100],"lines":{"startLine":section*5+1,"endLine":(section+1)*5}},
                "context":[{"bytes":[0,5],"lines":{"startLine":1,"endLine":1}},
                    {"bytes":[10,15],"lines":{"startLine":2,"endLine":2}}],"answers":answers})
        }).collect::<Vec<_>>();
        resources.insert(resource.into(), json!({"status":"ready",
            "source":{"tool":"localFetch","query":{"path":format!("{resource}.ts"),
                "ranges":[{"startLine":1,"endLine":2},{"startLine":4,"endLine":5}]}},
            "snapshot":format!("fixed-{resource}"),"coordinateSpace":"source",
            "read":{"kind":"continue","tool":"localFetch","input":{"cursor":"cursor1.pinned-source"}},
            "sections":sections,"coverage":{"totalSections":3,"completedSections":3,"remainingSections":0,
                "answered":21,"error":0,"unsupported":0,"unassessed":0}}));
    }
    let original = json!({"operation":"sections","resources":resources,"questions":questions,
        "next":{"nextSectionPage":{"kind":"continue","tool":"clasify","input":{"cursor":"cursor1.saved-assessment"}}}});
    assert_row_contract(ToolId::Clasify, &original);
    let plan = PreparedRows::new(
        vec![json!({"index":0,"data":original})],
        &[None],
        Some(ToolId::Clasify),
    )
    .plan(1_800, &CancellationToken::new())
    .expect("valid plan");
    assert!(
        plan.total_pages > 1,
        "fixture must exercise nested pagination"
    );
    let mut seen = BTreeMap::new();
    let (mut definitions, mut next_calls) = (0, 0);
    for page in 0..plan.total_pages {
        for row in plan
            .build_page(page, &[None], &[], &CancellationToken::new())
            .expect("valid build_page")
        {
            let data = &row["data"];
            assert_row_contract(ToolId::Clasify, data);
            definitions += usize::from(data.get("questions").is_some());
            if data["next"].get("nextSectionPage").is_some() {
                next_calls += 1;
                assert_eq!(row["rowPart"]["part"], row["rowPart"]["of"]);
            }
            for (key, resource) in data["resources"].as_object().expect("resources") {
                let expected_resource = &original["resources"][key];
                for field in ["source", "snapshot", "coordinateSpace", "read", "coverage"] {
                    assert_eq!(resource[field], expected_resource[field], "{key}/{field}");
                }
                for section in resource["sections"].as_array().expect("sections") {
                    let id = section["id"].as_str().expect("section id");
                    let expected_section = expected_resource["sections"]
                        .as_array()
                        .expect("source sections")
                        .iter()
                        .find(|item| item["id"] == id)
                        .expect("original section");
                    for field in ["kind", "target", "context"] {
                        assert_eq!(
                            section[field], expected_section[field],
                            "{key}/{id}/{field}"
                        );
                    }
                    for (question, answer) in section["answers"].as_object().expect("answers") {
                        assert!(
                            seen.insert(
                                (key.clone(), id.to_owned(), question.clone()),
                                answer.clone()
                            )
                            .is_none(),
                            "duplicate answer"
                        );
                    }
                }
            }
        }
    }
    assert_eq!(seen, expected);
    assert_eq!(definitions, 1);
    assert_eq!(next_calls, 1);
}
