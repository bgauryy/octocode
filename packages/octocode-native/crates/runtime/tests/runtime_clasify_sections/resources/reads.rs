use super::*;
use octocode_test_support::json_fixture;

/// Confirming a yes costs the lines it names: `next.read` reads exactly the
/// yes-sections' lines, pinned to the judged snapshot, not the whole source
/// the resource's own `read` returns.
#[tokio::test]
async fn next_read_reads_only_the_lines_of_the_yes_sections() {
    let provider = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(|request: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&request.body).expect("provider body");
            let yes = body["state"]["target"]["text"]
                .as_str()
                .is_some_and(|text| text.contains("restart 1"));
            let answers = body["questions"]
                .as_object()
                .expect("questions")
                .keys()
                .map(|id| {
                    (
                        id.clone(),
                        json!({"type":"noul","noul":if yes {0.9} else {0.1}}),
                    )
                })
                .collect::<serde_json::Map<_, _>>();
            ResponseTemplate::new(200).set_body_json(json!({
                "model":"jev-1.13.0","answers":answers,
                "usage":{"input_tokens":10,"output_tokens":2}
            }))
        })
        .mount(&provider)
        .await;
    let workspace = Workspace::new();
    workspace.write(
        "fns.rs",
        crate::runtime_clasify_sections::separate_functions(3),
    );
    let runtime = clasify_runtime(&workspace, &provider, &[]);
    let out = sections(
        &runtime,
        json_fixture(r#"{"a": {"tool": "localFetch", "query": {"path": "fns.rs"}}}"#),
    )
    .await;
    let resource = &out["resources"]["a"];
    let hit = resource["sections"]
        .as_array()
        .expect("sections")
        .iter()
        .find(|section| section["answers"]["q"]["answer"]["noul"] == json!(0.9))
        .unwrap_or_else(|| panic!("a yes section: {out:#}"));
    // The page shows the yes; the two no sections are held back, counted and
    // listed by `next.quiet` (hits view): every section still reaches the caller.
    let shown = resource["sections"].as_array().unwrap().len() as u64;
    let quiet = resource["coverage"]["quiet"].as_u64().unwrap_or(0);
    assert_eq!(
        shown + quiet,
        resource["coverage"]["totalSections"]
            .as_u64()
            .expect("total"),
        "{out:#}"
    );
    assert!(quiet >= 1, "the no sections are held back: {out:#}");
    let replay = runtime
        .run(
            "quiet".into(),
            "clasify".into(),
            out["next"]["quiet"]["input"].clone(),
        )
        .await
        .expect("next.quiet runs")
        .structured_content
        .into_value();
    let held = replay["results"][0]["data"]["resources"]["a"]["sections"]
        .as_array()
        .unwrap_or_else(|| panic!("quiet sections: {replay:#}"))
        .clone();
    assert_eq!(held.len() as u64, quiet, "{replay:#}");
    assert!(
        held.iter().all(
            |section| section["answers"]["q"]["answer"]["noul"] == json!(0.1)
                && section["id"] != hit["id"]
        ),
        "{replay:#}"
    );
    let read = &out["next"]["read"];
    assert_eq!(read["tool"], "localFetch", "{out:#}");
    let lines = hit["target"]["lines"].clone();
    // The continuation reads the yes-section's lines and no other function.
    let page = runtime
        .run("read".into(), "localFetch".into(), read["input"].clone())
        .await
        .expect("the continuation runs")
        .structured_content
        .into_value();
    let content = page["results"][0]["data"]["content"]
        .as_str()
        .unwrap_or_else(|| panic!("content: {page:#}"))
        .to_owned();
    assert!(content.contains("restart 1"), "{lines}: {content}");
    assert!(
        !content.contains("restart 0") && !content.contains("restart 2"),
        "only the yes-section's lines: {content}"
    );
    crate::runtime_clasify_sections::assert_next_is_only_pinned_reads(&out);
    assert!(out["next"].get("read2").is_none(), "{out:#}");
    runtime.close().await;
}

/// When every section said yes, the resource still gets a top-level
/// `next.read` of its selected lines: a page of many resources names each
/// one to read without its rows being searched.
#[tokio::test]
async fn next_read_is_offered_when_every_section_is_a_yes() {
    let provider = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(|request: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&request.body).expect("provider body");
            let answers = body["questions"]
                .as_object()
                .expect("questions")
                .keys()
                .map(|id| (id.clone(), json_fixture(r#"{"type": "noul", "noul": 0.9}"#)))
                .collect::<serde_json::Map<_, _>>();
            ResponseTemplate::new(200).set_body_json(json!({
                "model":"jev-1.13.0","answers":answers,
                "usage":{"input_tokens":10,"output_tokens":2}
            }))
        })
        .mount(&provider)
        .await;
    let workspace = Workspace::new();
    workspace.write(
        "fns.rs",
        crate::runtime_clasify_sections::separate_functions(3),
    );
    let runtime = clasify_runtime(&workspace, &provider, &[]);
    let out = sections(
        &runtime,
        json_fixture(r#"{"a": {"tool": "localFetch", "query": {"path": "fns.rs"}}}"#),
    )
    .await;
    let resource = &out["resources"]["a"];
    assert!(
        resource["sections"].as_array().unwrap().len() > 1,
        "{out:#}"
    );
    assert!(out["next"].get("read").is_some(), "{out:#}");
    runtime.close().await;
}
