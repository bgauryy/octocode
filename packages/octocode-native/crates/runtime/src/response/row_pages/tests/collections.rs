use super::*;
use tokio_util::sync::CancellationToken;

#[test]
fn declared_nested_collections_preserve_atomic_answers_and_shared_definitions() {
    let mut resources = Map::new();
    let mut expected = std::collections::BTreeMap::new();
    for resource in ["0", "r/~other"] {
        let sections = (0..3).map(|section| {
                let answers = (0..7).map(|question| {
                    let key = format!("q/{question}");
                    let probabilities = (0..13).map(|option| (format!("option-{option}"), json!(1.0 / 13.0))).collect::<Map<_, _>>();
                    let answer = json!({"status":"answered","answer":{
                        "type":"choice", "choice":"option-0", "confidence":0.0,
                        "probabilities":probabilities
                    }});
                    expected.insert((resource.to_owned(), format!("s{section}"), key.clone()), answer.clone());
                    (key, answer)
                }).collect::<Map<_, _>>();
                json!({"id":format!("s{section}"),"target":{"bytes":[section*100,(section+1)*100],"lines":[section*5+1,(section+1)*5]},
                    "context":[{"bytes":[0,5],"lines":[1,1]}],"answers":answers})
            }).collect::<Vec<_>>();
        resources.insert(
            resource.into(),
            json!({"status":"ready","snapshot":"fixed","sections":sections}),
        );
    }
    let row = json!({"id":"research","operation":"sections","resources":resources,
            "questions":(0..7).map(|question| (format!("q/{question}"), json!({"type":"choice","criteria":(0..13).map(|option| (format!("option-{option}"), Value::Null)).collect::<Map<_,_>>()}))).collect::<Map<_,_>>(),
            "next":{"nextSectionPage":{"tool":"clasify","query":{"queries":[{"operation":"sections","assessment":"next"}]}}}});
    let plan = PreparedRows::new(
        vec![json!({"index":0,"data":row})],
        &[None],
        crate::id::ToolId::Clasify,
    )
    .plan(1_800, &CancellationToken::new())
    .expect("valid plan");
    assert!(plan.total_pages > 1);
    let mut seen = std::collections::BTreeMap::new();
    let mut definitions = 0;
    let mut next_calls = 0;
    for page in 0..plan.total_pages {
        for row in plan
            .build_page(page, &[None], &[], &CancellationToken::new())
            .expect("valid build_page")
        {
            definitions += usize::from(row["data"].get("questions").is_some());
            next_calls += usize::from(row["data"]["next"].get("nextSectionPage").is_some());
            if row["data"]["next"].get("nextSectionPage").is_some() {
                assert_eq!(row["rowPart"]["part"], row["rowPart"]["of"]);
            }
            for (resource, value) in row["data"]["resources"].as_object().expect("resources") {
                for section in value["sections"].as_array().expect("sections") {
                    assert_eq!(
                        section["target"]["bytes"].as_array().expect("bytes").len(),
                        2
                    );
                    assert_eq!(
                        section["target"]["lines"].as_array().expect("lines").len(),
                        2
                    );
                    assert_eq!(section["context"][0]["bytes"], json!([0, 5]));
                    for (question, answer) in section["answers"].as_object().expect("answers") {
                        let key = (
                            resource.clone(),
                            section["id"].as_str().expect("id").to_owned(),
                            question.clone(),
                        );
                        assert!(
                            seen.insert(key, answer.clone()).is_none(),
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

#[test]
fn declared_collections_leave_a_single_oversized_answer_whole() {
    let probabilities = (0..100)
        .map(|index| (format!("option-{index}-{}", "x".repeat(48)), json!(0.01)))
        .collect::<Map<_, _>>();
    let answer = json!({"status":"answered","answer":{"type":"choice","choice":probabilities.keys().next().expect("option"),"confidence":0.0,"probabilities":probabilities}});
    let row = json!({"id":"research","operation":"sections","questions":{"q":{"type":"choice"}},
            "resources":{"r":{"sections":[{"id":"s","target":{"bytes":[0,1],"lines":[1,1]},"answers":{"q":answer}}]}}});
    let plan = PreparedRows::new(
        vec![json!({"index":0,"data":row})],
        &[None],
        crate::id::ToolId::Clasify,
    )
    .plan(500, &CancellationToken::new())
    .expect("valid plan");
    assert_eq!(plan.total_pages, 1);
    let rows = plan
        .build_page(0, &[None], &[], &CancellationToken::new())
        .expect("valid build_page");
    assert_eq!(
        rows[0]["data"]["resources"]["r"]["sections"][0]["answers"]["q"],
        answer
    );
}

#[test]
fn large_resource_identity_does_not_force_one_page_per_small_answer() {
    let answers = (0..25)
        .map(|index| {
            (
                format!("q{index}"),
                json!({
                    "status":"answered","answer":{"type":"noul","probability":0.5}
                }),
            )
        })
        .collect::<Map<_, _>>();
    let row = json!({"id":"research","operation":"sections",
            "resources":{"r":{"source":{"url":format!("https://example.test/{}","x".repeat(4000))},
            "sections":[{"id":"s","target":{"bytes":[0,1],"lines":[1,1]},"answers":answers}]}}});
    let plan = PreparedRows::new(
        vec![json!({"index":0,"data":row})],
        &[None],
        crate::id::ToolId::Clasify,
    )
    .plan(500, &CancellationToken::new())
    .expect("valid plan");
    assert_eq!(plan.total_pages, 1);
    let rows = plan
        .build_page(0, &[None], &[], &CancellationToken::new())
        .expect("valid build_page");
    assert_eq!(
        rows[0]["data"]["resources"]["r"]["sections"][0]["answers"],
        json!(answers)
    );
}

#[test]
fn answer_fragments_continue_their_only_section() {
    let answers = (0..30)
        .map(|index| {
            (
                format!("q{index}"),
                json!({
                    "status":"answered","answer":{"type":"noul","probability":0.5}
                }),
            )
        })
        .collect::<Map<_, _>>();
    let row = json!({"id":"research","operation":"sections",
            "resources":{"r/~0":{"sections":[{"id":"s","target":{"bytes":[0,1],"lines":[1,1]},"answers":answers}]}}});
    let plan = PreparedRows::new(
        vec![json!({"index":0,"data":row})],
        &[None],
        crate::id::ToolId::Clasify,
    )
    .plan(600, &CancellationToken::new())
    .expect("valid plan");
    assert!(plan.total_pages > 1);
    let mut seen = Map::new();
    let mut fragments = 0;
    for page in 0..plan.total_pages {
        for row in plan
            .build_page(page, &[None], &[], &CancellationToken::new())
            .expect("valid build_page")
        {
            if fragments > 0 {
                assert_eq!(
                    row["rowPart"]["continues"],
                    json!({"array":"/data/resources/r~1~00/sections","index":0})
                );
            }
            for (key, value) in row["data"]["resources"]["r/~0"]["sections"][0]["answers"]
                .as_object()
                .expect("answers")
            {
                assert!(seen.insert(key.clone(), value.clone()).is_none());
            }
            fragments += 1;
        }
    }
    assert_eq!(seen, answers);
}

#[test]
fn numeric_resource_keys_are_not_array_indexes() {
    let row = json!({"resources":{"0":{"sections":[{"answers":{}}]}}});
    assert_eq!(
        enclosing_element(&row, "/resources/0/sections/0/answers"),
        Some(("/resources/0/sections".into(), 0))
    );
}
