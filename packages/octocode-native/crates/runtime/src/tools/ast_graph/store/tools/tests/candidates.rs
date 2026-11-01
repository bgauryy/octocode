use super::*;

fn fixture() -> Fixture {
    let dir = Fixture::new();
    for id in 0..25 {
        dir.write(
            &format!("app/declaration-{id:02}.ts"),
            "export function SharedCollision() { return 1; }",
        );
        dir.write(
            &format!("app/dir-{id:02}/entry.ts"),
            "export const entry = 1;",
        );
    }
    dir.write("app/other.ts","export function SharedCollisionExtra() {} export function prefixSharedCollision() {} export function unique() {}");
    call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    dir
}

#[test]
fn ambiguity_continues_the_exact_normalized_population_after_its_preview() {
    let dir = fixture();
    for target in [
        "SharedCollision",
        "./SharedCollision",
        " ././SharedCollision ",
    ] {
        let first = call(
            &dir.0,
            ToolId::GraphQuery,
            json!({"operation":"node","path":"app","target":target}),
        );
        assert_eq!(first["errorCode"], "invalidInput");
        assert_eq!(first["pagination"]["totalItems"], 25);
        let preview = first["candidates"].as_array().unwrap();
        assert_eq!(preview.len(), 20);
        let mut ids = preview
            .iter()
            .map(|r| r["id"].as_str().unwrap().to_owned())
            .collect::<BTreeSet<_>>();
        let mut request = first["next"]["nextPage"]["input"]["queries"][0].clone();
        assert_eq!(request["operation"], "candidates");
        assert_eq!(request["target"], "SharedCollision");
        assert_eq!(request["offset"], 20);
        let graph = first["graph"]["id"].clone();
        loop {
            request["limit"] = json!(2);
            let page = call(&dir.0, ToolId::GraphQuery, request);
            assert_eq!(page["graph"]["id"], graph);
            assert_eq!(page["pagination"]["totalItems"], 25);
            for row in page["rows"].as_array().unwrap() {
                assert!(ids.insert(row["id"].as_str().unwrap().to_owned()));
            }
            match page.pointer("/next/nextPage/input/queries/0") {
                Some(next) => request = next.clone(),
                None => break,
            }
        }
        assert_eq!(ids.len(), 25);
        assert!(ids.iter().all(|id| id.ends_with("#SharedCollision")));
    }
}

#[test]
fn ambiguous_destinations_and_impact_seeds_retain_the_failed_reference_only() {
    let dir = fixture();
    for input in [
        json!({"operation":"path","target":"other.ts#unique","to":"./SharedCollision","edges":["calls"],"confidence":"low"}),
        json!({"operation":"witnesses","target":"other.ts#unique","to":"./SharedCollision","edges":["calls"],"limit":1}),
        json!({"operation":"impact","changed":["./SharedCollision"],"depth":2}),
    ] {
        let mut input = input;
        input["path"] = json!("app");
        let first = call(&dir.0, ToolId::GraphQuery, input);
        let request = &first["next"]["nextPage"]["input"]["queries"][0];
        assert_eq!(request["target"], "SharedCollision");
        assert_eq!(request["offset"], 20);
        for key in ["to", "edges", "confidence", "changed", "depth"] {
            assert!(request.get(key).is_none(), "{request}");
        }
        let rest = call(&dir.0, ToolId::GraphQuery, request.clone());
        assert_eq!(rest["rows"].as_array().unwrap().len(), 5);
    }
    let output = query(
        &QueryOptions {
            op: "issues".into(),
            workspace: Some(dir.0.clone()),
            entries: vec!["./entry.ts".into()],
            ..Default::default()
        },
        &workspace_policy(&dir.0),
    );
    assert_eq!(
        crate::tools::ast_graph::test_support::next_query(&output)
            .unwrap()
            .target
            .as_deref(),
        Some("entry.ts")
    );
}

#[test]
fn candidates_remain_pinned_when_a_new_snapshot_becomes_latest() {
    let dir = fixture();
    let first = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"node","path":"app","target":"SharedCollision"}),
    );
    dir.write("app/new.ts", "export function SharedCollision(){}");
    let rebuilt = call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    assert_ne!(rebuilt["id"], first["graph"]["id"]);
    let rest = call(
        &dir.0,
        ToolId::GraphQuery,
        first["next"]["nextPage"]["input"]["queries"][0].clone(),
    );
    assert_eq!(rest["pagination"]["totalItems"], 25);
    assert_eq!(rest["graph"]["id"], first["graph"]["id"]);
    let current = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"candidates","path":"app","target":"./SharedCollision"}),
    );
    assert_eq!(current["pagination"]["totalItems"], 26);
}

#[test]
fn candidate_exact_keys_empty_and_past_end_pages_validate() {
    let dir = fixture();
    for (target, total) in [
        ("other.ts#unique", 1),
        ("not present", 0),
        ("sharedcollision", 0),
    ] {
        let page = call(
            &dir.0,
            ToolId::GraphQuery,
            json!({"operation":"candidates","path":"app","target":target}),
        );
        assert_eq!(page["pagination"]["totalItems"], total);
    }
    let page = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"candidates","path":"app","target":"SharedCollision","offset":99,"limit":1}),
    );
    assert_eq!(page["pagination"]["totalItems"], 25);
    assert_eq!(page["rows"], json!([]));
    assert!(page.get("next").is_none());
}

#[test]
fn direct_and_public_call_queries_share_high_default_and_explicit_weak_levels() {
    let dir = Fixture::new();
    dir.write(
        "app/main.ts",
        "export function run(){ return candidateTarget(); }",
    );
    dir.write(
        "app/lib.ts",
        "export function candidateTarget(){ return 1; }",
    );
    call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    for confidence in [None, Some("high"), Some("medium"), Some("low")] {
        let direct = query(
            &QueryOptions {
                op: "callees".into(),
                workspace: Some(dir.0.clone()),
                target: Some("main.ts#run".into()),
                confidence: confidence.map(str::to_owned),
                ..Default::default()
            },
            &workspace_policy(&dir.0),
        );
        let mut input = json!({"operation":"callees","path":"app","target":"main.ts#run"});
        if let Some(level) = confidence {
            input["confidence"] = json!(level);
        }
        let public = call(&dir.0, ToolId::GraphQuery, input);
        assert_eq!(direct.value["rows"], public["rows"]);
        assert_eq!(direct.value["confidence"], public["confidence"]);
        assert_eq!(
            public["pagination"]["totalItems"],
            if matches!(confidence, Some("medium" | "low")) {
                1
            } else {
                0
            }
        );
    }
}

#[cfg(unix)]
#[test]
fn snapshot_absolute_candidate_keys_do_not_follow_changed_filesystem_aliases() {
    let dir = fixture();
    let file = dir.0.join("app/other.ts");
    let canonical = std::fs::canonicalize(&file).unwrap();
    let request =
        json!({"operation":"candidates","path":"app","target":canonical.to_str().unwrap()});
    let before = call(&dir.0, ToolId::GraphQuery, request.clone());
    assert_eq!(before["pagination"]["totalItems"], 1);
    assert_eq!(before["rows"][0]["id"], "other.ts");
    std::fs::remove_file(&file).unwrap();
    std::os::unix::fs::symlink(dir.0.join("app/declaration-00.ts"), &file).unwrap();
    let mut pinned = request;
    pinned["graph"] = before["graph"]["id"].clone();
    let after = call(&dir.0, ToolId::GraphQuery, pinned);
    assert_eq!(after["rows"], before["rows"]);
}

#[test]
fn a_missing_reference_offers_its_near_names_as_ready_calls() {
    let dir = Fixture::new();
    dir.write("app/a.ts", "export function shared_target(){}\n");
    call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    let row = json!({"operation":"callers","path":"app","target":"shared_targets"});
    let row = crate::contracts::validate_built_query(ToolId::GraphQuery.as_str(), row)
        .expect("input contract");
    let result = execute(ToolId::GraphQuery, &row, &dir.0, &context()).expect("execute");
    assert_eq!(result.data["errorCode"], "notFound", "{}", result.data);
    assert!(result.data.get("suggestions").is_none(), "{}", result.data);
    let related = result.data["related"].as_array().expect("related calls");
    assert!(!related.is_empty(), "{}", result.data);
    for call in related {
        assert_eq!(call["kind"], "related");
        assert_eq!(call["input"]["queries"][0]["operation"], "callers");
        assert_eq!(call["input"]["queries"][0]["target"], "a.ts#shared_target");
    }
    crate::tools::test_support::assert_row_contract(ToolId::GraphQuery, &result.data);
}
