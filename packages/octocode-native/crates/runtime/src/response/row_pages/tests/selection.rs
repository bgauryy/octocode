use super::*;

/// The plan a response gets is the one the earlier two-step choice made:
/// plan with the block of a finished page and keep it only when it is one
/// page, else plan again with the block of a page with more behind it. The
/// rows are now sized once ([`PreparedRows::size`]) and planned once; over
/// many shapes and budgets the pages are the same.
#[test]
fn the_reserve_is_chosen_before_planning_as_planning_twice_chose_it() {
    let mut seed = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = move |bound: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        usize::try_from(seed % bound as u64).expect("bounded")
    };
    let (mut one_page, mut split) = (0, 0);
    for case in 0..1_200 {
        let rows = (0..=next(4))
            .map(|index| {
                let matches = (0..next(12))
                    .map(|line| json!({"line":line,"text":"y".repeat(next(60))}))
                    .collect::<Vec<_>>();
                json!({"index":index,"data":{"files":[{"path":"a","matches":matches}],"note":"z".repeat(next(50))}})
            })
            .collect::<Vec<_>>();
        let volatile = (0..rows.len())
            .map(|_| (next(2) == 0).then(|| json!({"rateLimit":{"remaining":next(9)}})))
            .map(|fields| fields.and_then(|fields| fields.as_object().cloned()))
            .collect::<Vec<_>>();
        let mut structured = json!({"results":rows})
            .as_object()
            .cloned()
            .expect("object");
        if next(2) == 0 {
            structured.insert("warnings".into(), json!(["w".repeat(next(30))]));
        }
        let budget = 150 + next(2_500);
        let (finished, widest) = (3 + next(40), 40 + next(60));
        let block = |pagination: &ResponsePagination| {
            if pagination.has_more {
                widest
            } else {
                finished
            }
        };
        let options = ResponsePageOptions {
            response_length: Some(budget),
            response_scope: Some(crate::response::cursor::PageScope::Rows),
            ..Default::default()
        };
        let envelope = octocode_engine::digest::fingerprint_json(&structured).expect("envelope");
        let source = RowSource::new(
            structured.clone(),
            &envelope,
            &options,
            volatile.clone(),
            None,
            &block,
        );

        let mut skeleton = structured;
        let Some(Value::Array(rows)) = skeleton.remove("results") else {
            unreachable!("rows");
        };
        let overhead =
            octocode_engine::json_size::utf16_units(&skeleton).unwrap_or(0) + "results".len() + 6;
        let banner = incomplete_banner_chars(&skeleton, rows.len());
        let row_budget = |reserve: usize| budget.saturating_sub(overhead + banner + reserve).max(1);
        let prepared = PreparedRows::new(rows, &volatile, None);
        let earlier = if envelope.utf16_units + finished <= budget {
            let plan = prepared.clone().plan(row_budget(finished));
            if plan.total_pages == 1 {
                plan
            } else {
                prepared.plan(row_budget(widest))
            }
        } else {
            prepared.plan(row_budget(widest))
        };
        assert_eq!(source.plan.total_pages, earlier.total_pages, "case {case}");
        for page in 0..earlier.total_pages {
            assert_eq!(
                source.plan.build_page(page, &source.volatile, &[]),
                earlier.build_page(page, &volatile, &[]),
                "case {case} page {page}"
            );
        }
        if earlier.total_pages == 1 {
            one_page += 1
        } else {
            split += 1
        }
    }
    assert!(
        one_page > 50 && split > 50,
        "{one_page} one-page, {split} split cases"
    );
}
