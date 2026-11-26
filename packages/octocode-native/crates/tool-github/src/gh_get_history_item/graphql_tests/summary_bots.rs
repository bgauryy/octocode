//! A summary counts what bots wrote in each discussion section, from the
//! one GraphQL read that states its whole comment count.
use super::*;

fn entry(login: &str, kind: &str, body: &str) -> Value {
    json!({"author": {"login": login, "__typename": kind}, "body": body})
}

fn connection(total: usize, nodes: &[Value]) -> Value {
    json!({"totalCount": total, "nodes": nodes})
}

/// The bot-written entries of `nodes` and their bodies' characters.
fn bots_of(nodes: &[Value]) -> (usize, usize) {
    let bots = nodes
        .iter()
        .filter(|node| node["author"]["__typename"] == "Bot");
    (
        bots.clone().count(),
        bots.map(|node| node["body"].as_str().unwrap_or("").chars().count())
            .sum(),
    )
}

#[tokio::test]
async fn a_summary_counts_bot_entries_per_section_in_its_one_graphql_read() {
    let comments = [
        entry("amy", "User", "why this way?"),
        entry("coderabbitai", "Bot", &"walkthrough ".repeat(40)),
        entry("pkg-pr-new", "Bot", "published é"),
    ];
    let reviews = [
        entry("rev", "User", "LGTM"),
        entry("coderabbitai", "Bot", &"review body ".repeat(9)),
    ];
    let thread_a = [
        entry("cubic-dev-ai", "Bot", "possible null deref"),
        entry("rev", "User", "handled above"),
    ];
    let thread_b = [entry("rev", "User", "rename this")];
    let server = MockServer::builder().start().await;
    // One GraphQL request serves the count and the sections' authors.
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .and(body_string_contains("totalCommentsCount"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {"repository": {"pullRequest": {
                "totalCommentsCount": 8,
                // The conversation holds more than the page GraphQL listed.
                "comments": connection(comments.len() + 40, &comments),
                "reviews": connection(reviews.len(), &reviews),
                "reviewThreads": connection(2, &[
                    json!({"comments": connection(thread_a.len(), &thread_a)}),
                    json!({"comments": connection(thread_b.len(), &thread_b)}),
                ]),
            }}}
        })))
        .expect(1)
        .mount(&server)
        .await;
    mount_rest_pr(&server, 1).await;
    let summary = json!({"operation": "pullRequest", "owner": "a", "repo": "b", "number": 7});
    let out = read(&server, summary).await;
    let pr = &out.data["pullRequests"][0];
    let stated = &pr["botEntries"];
    let inline = [thread_a.as_slice(), thread_b.as_slice()].concat();
    for (section, nodes, whole) in [
        ("comments", comments.as_slice(), false),
        ("reviews", reviews.as_slice(), true),
        ("reviewComments", inline.as_slice(), true),
    ] {
        let (count, chars) = bots_of(nodes);
        assert_eq!(stated[section]["count"], count, "{section}: {pr}");
        assert_eq!(stated[section]["chars"], chars, "{section}: {pr}");
        // A section listed in part says how many entries the count covers.
        let counted = stated[section].get("counted").and_then(Value::as_u64);
        assert_eq!(
            counted,
            (!whole).then_some(nodes.len() as u64),
            "{section}: {pr}"
        );
    }
    assert_eq!(pr["totalCommentsCount"], 8, "{pr}");
    octocode_runtime_base::test_support::validate_sealed(
        "ghGetHistoryItem",
        &json!({"results": [octocode_runtime_base::response::rows::result_row(
            0,
            out.data.clone(),
            None,
        )]}),
    )
    .expect("the summary row satisfies the output contract");
}

/// A summary where no bot wrote states no `botEntries`.
#[tokio::test]
async fn a_summary_without_bot_entries_states_none() {
    let people = [entry("amy", "User", "why?")];
    let server = MockServer::builder().start().await;
    mount_graphql(
        &server,
        json!({"data": {"repository": {"pullRequest": {
            "totalCommentsCount": 1,
            "comments": connection(1, &people),
            "reviews": connection(0, &[]),
            "reviewThreads": connection(0, &[]),
        }}}}),
    )
    .await;
    mount_rest_pr(&server, 1).await;
    let summary = json!({"operation": "pullRequest", "owner": "a", "repo": "b", "number": 7});
    let out = read(&server, summary).await;
    let pr = &out.data["pullRequests"][0];
    assert!(pr.get("botEntries").is_none(), "{pr}");
    assert_eq!(pr["totalCommentsCount"], 1, "{pr}");
}
