//! Capped hierarchy walks: direct results kept, node caps that resume
//! through an executable continuation, independent of server order.

use super::*;

#[tokio::test(flavor = "current_thread")]
async fn the_anchor_keeps_every_direct_result_and_capped_parents_resume() {
    let (_dir, root, paths) = temp_workspace();
    let (file, uri) = graph_ts(&root);
    let wide = |prefix: &str| {
        (0..MAX_HIERARCHY_FAN_OUT + 10)
            .map(|index| format!("{prefix}{index}"))
            .collect::<Vec<_>>()
    };
    // The anchor's own results are paged, never cut.
    let mut flat_graph = graph(&[]);
    flat_graph.uri = Some(uri.clone());
    flat_graph.callers.insert("root".into(), wide("caller"));
    let flat = walk_from(
        &flat_graph,
        node_at("root", Some(&uri)),
        1,
        &paths,
        &crate::execution::cancel::NeverCancel,
    )
    .await
    .expect("walk");
    assert_eq!(flat.edges.len(), MAX_HIERARCHY_FAN_OUT + 10);
    assert!(flat.fan_out_capped.is_empty() && flat.resumes.is_empty());

    // A wide node below the anchor keeps the first results and resumes from
    // itself, where it is the anchor and lists every result.
    let mut deep_graph = graph(&[("root", &["hub"])]);
    deep_graph.uri = Some(uri.clone());
    deep_graph.callers.insert("hub".into(), wide("caller"));
    let deep = walk_from(
        &deep_graph,
        node_at("root", Some(&uri)),
        2,
        &paths,
        &crate::execution::cancel::NeverCancel,
    )
    .await
    .expect("walk");
    assert_eq!(deep.edges.len(), 1 + MAX_HIERARCHY_FAN_OUT);
    assert_eq!(deep.fan_out_capped, vec!["hub".to_owned()]);
    let q = query(serde_json::json!({
        "operation": "callers", "path": file.to_string_lossy(),
        "symbolName": "root",
        "line": 1,
        "graphDepth": 2
    }));
    let items = deep
        .edges
        .iter()
        .map(|edge| public_edge(Expansion::IncomingCalls, edge, None))
        .collect::<Vec<_>>();
    // One page: the parents and their continuations go on the last page.
    let mut row = items_payload(&q, "callers", serde_json::json!(items), 1_000_000);
    assert!(row["next"].get("nextPage").is_none(), "{}", row["next"]);
    mark_truncation(&mut row, &q, &[(Expansion::IncomingCalls, &deep)]);
    assert_eq!(row["clipped"], true);
    assert_eq!(row["isPartial"], true);
    assert!(row.get("terminalLimit").is_none(), "{row}");
    assert_eq!(
        row["partialReasons"],
        serde_json::json!(["hierarchyFanOutLimit"])
    );
    assert_eq!(row["unexpandedParents"][0]["symbolName"], "hub");
    let resume = &row["next"]["continueWalk"]["input"]["queries"][0];
    assert_eq!(resume["graphDepth"], 1, "{resume}");
    // Re-anchored by name and 1-based line, never a 0-based position.
    assert!(resume.get("position").is_none(), "{resume}");
    assert_eq!(resume["symbolName"], "hub", "{resume}");
    assert_eq!(
        resume["line"].as_u64(),
        node_at("hub", Some(&uri))["selectionRange"]["start"]["line"]
            .as_u64()
            .map(|line| line + 1),
        "{resume}"
    );
    let row = with_next(&q, row);
    crate::contracts::validate_output("lspSearch", &public_row(&row))
        .expect("fan-out-limited row satisfies the output contract");
    let resumed = walk_from(
        &deep_graph,
        node_at("hub", Some(&uri)),
        1,
        &paths,
        &crate::execution::cancel::NeverCancel,
    )
    .await
    .expect("resumed walk");
    assert_eq!(resumed.edges.len(), MAX_HIERARCHY_FAN_OUT + 10);
}

#[tokio::test(flavor = "current_thread")]
async fn hierarchy_node_cap_truncates_with_an_executable_continuation() {
    let (_dir, root, paths) = temp_workspace();
    let (file, uri) = graph_ts(&root);
    // 40 direct callers, each with 40 callers of its own: 1 + 40 + 1600
    // nodes within depth 2, far past the node cap.
    let mut graph = graph(&[]);
    graph.uri = Some(uri.clone());
    let level1 = (0..40).map(|index| format!("p{index}")).collect::<Vec<_>>();
    for parent in &level1 {
        graph.callers.insert(
            parent.clone(),
            (0..40).map(|index| format!("{parent}c{index}")).collect(),
        );
    }
    graph.callers.insert("root".into(), level1);
    let walk = walk_from(
        &graph,
        node_at("root", Some(&uri)),
        2,
        &paths,
        &crate::execution::cancel::NeverCancel,
    )
    .await
    .expect("walk");
    assert_eq!(
        walk.edges.len(),
        MAX_HIERARCHY_NODES - 1,
        "root + edges fill the cap"
    );
    assert!(walk.dropped_edges > 0);
    assert_eq!(graph.requests.get(), 41, "each node is expanded once");
    let resume = walk.resumes.first().expect("resume point");
    assert_eq!(resume.depth, 1);
    // Every parent whose children were dropped is recorded once.
    let names = walk
        .resumes
        .iter()
        .map(|resume| resume.node["name"].as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    assert!(names.len() > 1, "{names:?}");
    assert_eq!(
        names.iter().collect::<std::collections::HashSet<_>>().len(),
        names.len(),
        "no duplicate parents"
    );

    let q = query(serde_json::json!({
        "operation": "callers", "path": file.to_string_lossy(),
        "symbolName": "root",
        "line": 1,
        "graphDepth": 2
    }));
    let items = walk
        .edges
        .iter()
        .map(|edge| public_edge(Expansion::IncomingCalls, edge, None))
        .collect::<Vec<_>>();
    let mut row = items_payload(&q, "callers", serde_json::json!(items), TEST_BUDGET);
    mark_truncation(&mut row, &q, &[(Expansion::IncomingCalls, &walk)]);
    // Every page is clipped and warns; only the last lists the parents and
    // their continuations, so pages never repeat them.
    let pages = row["pagination"]["totalPages"].as_u64().expect("pages");
    assert!(pages > 1, "{}", row["pagination"]);
    assert_eq!(row["clipped"], true);
    assert!(row.get("unexpandedParents").is_none(), "{row}");
    assert!(row["next"].get("continueWalk").is_none(), "{}", row["next"]);
    let warning = row["warnings"][0].as_str().unwrap_or_default();
    assert!(
        warning.contains(&format!("{} parents", walk.resumes.len()))
            && warning.contains("last page")
            && !warning.contains(" 0 "),
        "{warning}"
    );
    let last = requery(&q, serde_json::json!({ "page": pages }));
    let mut row = items_payload(&last, "callers", serde_json::json!(items), TEST_BUDGET);
    mark_truncation(&mut row, &last, &[(Expansion::IncomingCalls, &walk)]);
    assert!(row["next"].get("nextPage").is_none(), "{}", row["next"]);
    assert_eq!(row["clipped"], true);
    assert_eq!(
        row["partialReasons"],
        serde_json::json!(["hierarchyNodeLimit"])
    );
    assert!(row.get("terminalLimit").is_none(), "{}", row["next"]);
    let resume_query = &row["next"]["continueWalk"]["input"]["queries"][0];
    assert_eq!(resume_query["graphDepth"], 1);
    assert_eq!(resume_query["path"], file.to_string_lossy().as_ref());
    // Walk continuations re-anchor by symbol and 1-based line.
    assert!(resume_query.get("position").is_none(), "{resume_query}");
    assert_eq!(resume_query["symbolName"], resume.node["name"]);
    assert_eq!(
        resume_query["line"].as_u64(),
        resume.node["selectionRange"]["start"]["line"]
            .as_u64()
            .map(|line| line + 1)
    );
    let parents = row["unexpandedParents"]
        .as_array()
        .expect("unexpanded parents");
    assert_eq!(parents.len(), walk.resumes.len());
    assert_eq!(parents[0]["remainingDepth"], 1);
    let resumed = (2..=walk.resumes.len())
        .map(|index| format!("continueWalk{index}"))
        .collect::<Vec<_>>();
    for key in &resumed {
        assert_eq!(row["next"][key]["tool"], "lspSearch", "{key}");
    }
    assert!(
        row["next"]
            .get(format!("continueWalk{}", walk.resumes.len() + 1))
            .is_none()
    );
    let row = with_next(&q, row);
    crate::contracts::validate_output("lspSearch", &public_row(&row))
        .expect("node-capped row satisfies the output contract");
}

/// Which nodes a node-capped walk lists does not depend on the order the
/// server answered in: two servers listing the same callers in opposite
/// orders walk the same nodes and drop the same edges.
#[tokio::test(flavor = "current_thread")]
async fn node_capped_walk_does_not_depend_on_server_order() {
    let (_dir, root, paths) = temp_workspace();
    let (_file, uri) = graph_ts(&root);
    let capped = |reversed: bool| {
        let mut graph = graph(&[]);
        graph.uri = Some(uri.clone());
        let mut level1 = (0..40).map(|index| format!("p{index}")).collect::<Vec<_>>();
        for parent in &level1 {
            let mut callers = (0..40)
                .map(|index| format!("{parent}c{index}"))
                .collect::<Vec<_>>();
            if reversed {
                callers.reverse();
            }
            graph.callers.insert(parent.clone(), callers);
        }
        if reversed {
            level1.reverse();
        }
        graph.callers.insert("root".into(), level1);
        graph
    };
    let mut walked = Vec::new();
    for reversed in [false, true] {
        let graph = capped(reversed);
        let walk = walk_from(
            &graph,
            node_at("root", Some(&uri)),
            2,
            &paths,
            &crate::execution::cancel::NeverCancel,
        )
        .await
        .expect("walk");
        assert!(walk.dropped_edges > 0, "the walk hits the node cap");
        let mut names = walk
            .edges
            .iter()
            .map(|edge| edge.node["name"].as_str().unwrap_or_default().to_owned())
            .collect::<Vec<_>>();
        names.sort();
        walked.push((names, walk.dropped_edges));
    }
    assert_eq!(walked[0], walked[1]);
}

/// A walk whose last level reached `graphDepth` left those nodes
/// unexpanded: `next.walkDeeper` reruns it one level deeper, without marking
/// the answer partial. A walk that ran out of nodes first, or one at the
/// maximum depth, offers nothing.
#[tokio::test(flavor = "current_thread")]
async fn a_walk_at_its_depth_offers_the_next_level() {
    let (_dir, root, paths) = temp_workspace();
    let (file, uri) = graph_ts(&root);
    let mut chain = graph(&[("root", &["child"]), ("child", &["grandchild"])]);
    chain.uri = Some(uri.clone());
    let row_at = |depth: u32, walk: &HierarchyWalk| {
        let q = query(serde_json::json!({
            "operation": "callers", "path": file.to_string_lossy(),
            "symbolName": "root",
            "line": 1,
            "graphDepth": depth
        }));
        let items = walk
            .edges
            .iter()
            .map(|edge| public_edge(Expansion::IncomingCalls, edge, None))
            .collect::<Vec<_>>();
        let mut row = items_payload(&q, "callers", serde_json::json!(items), TEST_BUDGET);
        mark_truncation(&mut row, &q, &[(Expansion::IncomingCalls, walk)]);
        row
    };
    let walk = |depth: u32| {
        walk_from(
            &chain,
            node_at("root", Some(&uri)),
            depth,
            &paths,
            &crate::execution::cancel::NeverCancel,
        )
    };

    let shallow = walk(1).await.expect("walk");
    let row = row_at(1, &shallow);
    let deeper = &crate::tools::test_support::checks::related_call(&row, "lspSearch", |input| {
        input["queries"][0]["graphDepth"] == 2
    })["input"]["queries"][0];
    assert_eq!(deeper["graphDepth"], 2, "{}", row["next"]);
    assert_eq!(deeper["symbolName"], "root");
    assert!(row.get("isPartial").is_none(), "{row}");

    // Depth 3 walks past the grandchild, which has no callers: complete.
    let whole = walk(3).await.expect("walk");
    assert_eq!(whole.edges.len(), 2);
    let row = row_at(3, &whole);
    assert!(
        !row["related"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|call| call["tool"] == "lspSearch"
                && call["input"]["queries"][0]["graphDepth"] == 2),
        "{}",
        row["next"]
    );

    // The deepest walk the contract allows has no deeper level to offer.
    let deepest = walk(MAX_HIERARCHY_DEPTH).await.expect("walk");
    let mut long = deepest;
    for edge in &mut long.edges {
        edge.level = MAX_HIERARCHY_DEPTH;
    }
    let row = row_at(MAX_HIERARCHY_DEPTH, &long);
    assert!(
        !row["related"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|call| call["tool"] == "lspSearch"
                && call["input"]["queries"][0]["graphDepth"] == 2),
        "{}",
        row["next"]
    );
}
