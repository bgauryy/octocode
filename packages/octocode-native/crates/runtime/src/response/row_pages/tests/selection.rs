use super::*;
use tokio_util::sync::CancellationToken;

/// Vary row shapes, volatile facts and page budgets while preserving every
/// ordered match and independent row context. Page boundaries may improve.
#[test]
fn budget_selection_preserves_every_row_across_varied_shapes() {
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
            response_chars: Some(budget),
            paging: true,
            ..Default::default()
        };
        let envelope = octocode_engine::digest::fingerprint_json(&structured).expect("envelope");
        let source = RowSource::new(
            structured.clone(),
            &envelope,
            &options,
            volatile.clone(),
            crate::id::ToolId::LocalSearch,
            &block,
            &CancellationToken::new(),
        )
        .expect("valid source");

        let originals = structured["results"].as_array().expect("rows");
        let mut seen = vec![Vec::new(); originals.len()];
        let mut parts = vec![Vec::new(); originals.len()];
        let mut provider_facts = vec![0; originals.len()];
        for page in 0..source.plan.total_pages {
            for row in source
                .plan
                .build_page(page, &volatile, &[], &CancellationToken::new())
                .expect("page")
            {
                let index = row["index"].as_u64().expect("index") as usize;
                let part = row["rowPart"]["part"].as_u64().unwrap_or(1);
                let of = row["rowPart"]["of"].as_u64().unwrap_or(1);
                parts[index].push((part, of));
                assert_eq!(
                    row["data"]["note"], originals[index]["data"]["note"],
                    "case {case}"
                );
                for file in row["data"]["files"].as_array().expect("files") {
                    assert_eq!(file["path"], "a");
                    seen[index]
                        .extend(file["matches"].as_array().expect("matches").iter().cloned());
                }
                if let Some(facts) = row["data"].get("rateLimit") {
                    assert_eq!(part, 1, "provider facts belong to the first/whole part");
                    assert_eq!(
                        Some(facts),
                        volatile[index]
                            .as_ref()
                            .and_then(|value| value.get("rateLimit"))
                    );
                    provider_facts[index] += 1;
                }
            }
        }
        for index in 0..originals.len() {
            let of = parts[index].first().expect("each row is present").1;
            assert_eq!(
                parts[index],
                (1..=of).map(|part| (part, of)).collect::<Vec<_>>(),
                "case {case} row {index} parts must occur exactly once in order"
            );
            assert_eq!(
                seen[index],
                originals[index]["data"]["files"][0]["matches"]
                    .as_array()
                    .unwrap()
                    .to_vec(),
                "case {case} row {index}"
            );
            assert_eq!(
                provider_facts[index],
                usize::from(volatile[index].is_some())
            );
        }
        if source.plan.total_pages == 1 {
            one_page += 1;
        } else {
            split += 1;
        }
    }
    assert!(
        one_page > 50 && split > 50,
        "{one_page} one-page, {split} split cases"
    );
}
