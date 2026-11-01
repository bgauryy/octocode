use super::*;
use crate::tools::ast_graph::store::{QueryOptions, query};

fn fixture() -> Fixture {
    let dir = Fixture::new();
    dir.write(
        "app/a.ts",
        "import { ext } from 'pkg';\nfunction local(){}\nexport function a(){ missing(); ext(); local(); gone(); }\n",
    );
    dir.write("app/b.ts", "export function b(){ absent(); ext2(); }\n");
    dir.write("app/c.ts", "export function c(){ vanished(); }\n");
    call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    dir
}

fn ids(page: &Value) -> Vec<String> {
    page["rows"]
        .as_array()
        .expect("results")
        .iter()
        .map(|row| row["id"].as_str().expect("id").to_owned())
        .collect()
}

/// Every row of `query`, following `next` to the end.
fn all_pages(dir: &Path, mut query: Value) -> (Vec<Value>, u64) {
    let mut rows = Vec::new();
    loop {
        let page = call(dir, ToolId::GraphQuery, query);
        rows.extend(page["rows"].as_array().expect("results").iter().cloned());
        match page.pointer("/next/nextPage/input/queries/0") {
            Some(next) => query = next.clone(),
            None => {
                return (
                    rows,
                    page["pagination"]["totalItems"].as_u64().expect("total"),
                );
            }
        }
    }
}

#[test]
fn unresolved_reasons_filter_before_paging_and_count_the_kept_rows() {
    let dir = fixture();
    let all = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"unresolved","path":"app","limit":1000}),
    );
    let rows = all["rows"].as_array().expect("rows");
    let reasons = rows
        .iter()
        .map(|row| row["reason"].as_str().expect("reason"))
        .collect::<BTreeSet<_>>();
    assert!(reasons.len() >= 2, "{all}");
    let wanted = *reasons.first().expect("reason");
    let expected = rows
        .iter()
        .filter(|row| row["reason"] == wanted)
        .map(|row| row["id"].as_str().expect("id").to_owned())
        .collect::<Vec<_>>();
    let (kept, total) = all_pages(
        &dir.0,
        json!({"operation":"unresolved","path":"app","limit":1,"reasons":[wanted]}),
    );
    assert_eq!(total, expected.len() as u64);
    assert_eq!(
        kept.iter()
            .map(|row| row["id"].as_str().expect("id").to_owned())
            .collect::<Vec<_>>(),
        expected,
        "the filter keeps snapshot order across pages"
    );
    assert!(kept.iter().all(|row| row["reason"] == wanted));
    let every = reasons
        .iter()
        .map(|reason| json!(reason))
        .collect::<Vec<_>>();
    let unfiltered = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"unresolved","path":"app","limit":1000,"reasons":every}),
    );
    assert_eq!(ids(&unfiltered), ids(&all));
}

#[test]
fn evidence_pages_take_several_sources_and_return_rows_from_any() {
    let dir = fixture();
    for operation in ["unresolved", "witnesses"] {
        let ask = |target: Value| {
            call(
                &dir.0,
                ToolId::GraphQuery,
                json!({"operation":operation,"path":"app","limit":1000,"target":target}),
            )
        };
        let a = ask(json!("a.ts"));
        let b = ask(json!("b.ts"));
        let both = ask(json!(["a.ts", "b.ts"]));
        assert!(!ids(&a).is_empty() && !ids(&b).is_empty(), "{operation}");
        assert_eq!(
            both["pagination"]["totalItems"].as_u64(),
            Some(
                a["pagination"]["totalItems"].as_u64().unwrap()
                    + b["pagination"]["totalItems"].as_u64().unwrap()
            )
        );
        let union = ids(&a).into_iter().chain(ids(&b)).collect::<BTreeSet<_>>();
        assert_eq!(ids(&both).into_iter().collect::<BTreeSet<_>>(), union);
        assert_eq!(ask(json!(["a.ts"]))["rows"], a["rows"]);
        let (paged, total) = all_pages(
            &dir.0,
            json!({"operation":operation,"path":"app","limit":1,"target":["a.ts","b.ts"]}),
        );
        assert_eq!(total, both["pagination"]["totalItems"].as_u64().unwrap());
        assert_eq!(
            paged
                .iter()
                .map(|row| row["id"].as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
            ids(&both)
        );
    }
}

#[test]
fn engine_continuations_carry_sources_reasons_and_the_provenance_choice() {
    let dir = fixture();
    let ask = |tweak: &dyn Fn(&mut QueryOptions)| {
        let mut options = QueryOptions {
            op: "unresolved".into(),
            workspace: Some(dir.0.clone()),
            limit: Some(1),
            ..Default::default()
        };
        tweak(&mut options);
        query(&options, &workspace_policy(&dir.0))
    };
    let page = ask(&|o| {
        o.targets = vec!["a.ts".into(), "b.ts".into()];
        o.reasons = vec!["not-declared-in-repo".into()];
        o.include_provenance = true;
    });
    // The next page resumes this same request: its filters and provenance.
    use crate::tools::ast_graph::store::{Channel, Resume};
    assert!(matches!(
        page.next.as_slice(),
        [(Channel::NextPage, Resume::Row { list: None, cycle: None, .. })]
    ));
    assert!(page.value["rows"][0]["derivation"].is_object());
    let lean = ask(&|o| o.targets = vec!["a.ts".into()]);
    assert!(lean.value["rows"][0].get("derivation").is_none());
    for (tweak, message) in [
        (
            &(|o: &mut QueryOptions| o.reasons = vec!["nope".into()]) as &dyn Fn(&mut QueryOptions),
            "unknown reason",
        ),
        (
            &|o: &mut QueryOptions| {
                o.op = "find".into();
                o.target = Some("a".into());
                o.reasons = vec!["other".into()];
            },
            "reasons applies to unresolved",
        ),
        (
            &|o: &mut QueryOptions| {
                o.op = "callers".into();
                o.targets = vec!["a.ts".into(), "b.ts".into()];
            },
            "takes one target",
        ),
    ] {
        let refused = ask(tweak);
        assert_eq!(
            refused.value["errorCode"], "invalidInput",
            "{}",
            refused.value
        );
        assert!(
            refused.value["error"].as_str().unwrap().contains(message),
            "{}",
            refused.value
        );
    }
}
