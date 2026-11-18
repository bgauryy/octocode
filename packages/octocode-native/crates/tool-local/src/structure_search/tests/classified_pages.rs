//! A classified later page of a listing: the real executor's page-2 query is
//! judged, and the `next.unfiltered` lead it offers reads exactly that page.
use super::*;
use octocode_tools_shared::selection::apply_sanitized_rows;
use octocode_tools_shared::test_support::selection_fake::{self, WordJudge};

fn listed_paths(data: &Value) -> Vec<String> {
    paths(data)
}

#[test]
fn the_unfiltered_lead_of_a_later_page_reads_exactly_the_entries_that_page_dropped() {
    let root = Fixture::new();
    for name in [
        "a_keep.rs",
        "b_drop.rs",
        "c_keep.rs",
        "d_drop.rs",
        "e_keep.rs",
        "f_drop.rs",
    ] {
        root.write(name, "x\n");
    }
    let classified = json!({"operation":"files","path":root.0,"entryType":"file",
        "classification":{"predicates":["Is it kept?"]}});
    with_window(3, || {
        let first = run(&root.0, classified).expect("page 1");
        let page_two = first["next"]["nextPage"]["input"]["queries"][0].clone();
        assert!(page_two.get("classification").is_some(), "{page_two}");
        let page = run(&root.0, page_two.clone()).expect("page 2");
        let on_page = listed_paths(&page);
        assert_eq!(on_page.len(), 3, "{page}");

        let executor = tokio::runtime::Runtime::new().expect("runtime");
        let runtime = selection_fake::runtime(
            WordJudge { fail: None },
            executor.handle().clone(),
            std::time::Duration::from_secs(5),
        );
        let mut structured = json!({"results":[{"index":0,"data":page}]});
        let mut response_query = page_two.clone();
        apply_sanitized_rows(
            octocode_runtime_base::id::ToolId::StructureSearch,
            &crate::structure_search::Output,
            &mut structured,
            &[octocode_runtime_base::security::CheckedQuery::checked(
                page_two.clone(),
            )],
            &[0],
            &mut response_query,
            &runtime,
        )
        .expect("classified page");
        let data = &structured["results"][0]["data"];
        let kept = listed_paths(data);
        assert!(!kept.is_empty() && kept.len() < on_page.len(), "{data}");
        assert!(kept.iter().all(|path| path.contains("keep")), "{kept:?}");

        // The lead is run as a caller runs it: through the executor.
        let lead = data["next"]["unfiltered"]["input"]["queries"][0].clone();
        assert!(lead.get("classification").is_none(), "{lead}");
        let walked = listed_paths(&run(&root.0, lead).expect("unfiltered page"));
        assert_eq!(walked, on_page, "the lead restarts elsewhere than its page");
        let dropped = walked
            .iter()
            .filter(|path| !kept.contains(path))
            .collect::<Vec<_>>();
        assert!(
            dropped.iter().all(|path| path.contains("drop")),
            "{dropped:?}"
        );
        let mut union = kept
            .iter()
            .chain(dropped.iter().copied())
            .collect::<Vec<_>>();
        union.sort();
        let mut expected = on_page.iter().collect::<Vec<_>>();
        expected.sort();
        assert_eq!(
            union, expected,
            "kept and dropped rebuild the page, once each"
        );
    });
}
