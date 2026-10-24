use super::*;

#[tokio::test]
async fn date_chunks_keep_the_query_and_return_every_entry_once() {
    let server = server().await;
    let entries = (0..320)
        .map(|index| {
            let name = format!("f{index:03}.rs");
            json!({"name":name,"path":name,"type":"file","size":1})
        })
        .collect::<Vec<_>>();
    mount_json(&server, "/api/v3/repos/a/b/contents", 200, json!(entries)).await;
    let mut object = serde_json::Map::new();
    object.insert("committedDate".into(), json!("2026-10-24T00:00:00Z"));
    for index in 0..100 {
        object.insert(
            format!("p{index}"),
            json!({"nodes":[{"committedDate":"2026-10-23T00:00:00Z"}]}),
        );
    }
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"data":{"repository":{"object":object}}})),
        )
        .mount(&server)
        .await;
    let mut row = json!({"operation":"tree","owner":"a","repo":"b","ref":"main","maxDepth":1});
    let mut names = Vec::new();
    let mut steps = 0;
    loop {
        let out = list(&server, row).await.expect("tree chunk");
        steps += 1;
        for directory in out.data["files"].as_array().expect("directories") {
            names.extend(
                directory["files"]
                    .as_array()
                    .expect("files")
                    .iter()
                    .map(|value| value.as_str().expect("entry").to_owned()),
            );
        }
        let next = out
            .data
            .pointer("/next/expandDates/input/queries/0")
            .or_else(|| out.data.pointer("/next/nextPage/input/queries/0"));
        let Some(next) = next else { break };
        assert_eq!(next["operation"], "tree");
        assert_eq!(next["owner"], "a");
        assert_eq!(next["repo"], "b");
        assert_eq!(next["ref"], SHA);
        assert_eq!(next["maxDepth"], 1);
        assert!(steps < 5, "nonterminating tree: {}", out.data);
        row = next.clone();
    }
    assert_eq!(steps, 4);
    assert_eq!(names.len(), 320);
    let expected = (0..320)
        .map(|index| format!("f{index:03}.rs (1, 2026-10-23)"))
        .collect::<Vec<_>>();
    assert_eq!(names, expected);
}
