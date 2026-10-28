use super::evidence_closure::{catalog, chars, exact_closure, origin};
use super::*;
use crate::contracts::output_fields::CollectionRoutes;
use std::sync::OnceLock;

fn routes() -> &'static CollectionRoutes {
    static ROUTES: OnceLock<CollectionRoutes> = OnceLock::new();
    ROUTES
        .get_or_init(|| CollectionRoutes::new(&["/data/primary".into(), "/data/peer".into()], &[]))
}

#[test]
fn short_peer_evidence_is_priced_with_its_large_descriptor_and_delivered_once() {
    for large in [false, true] {
        let mut descriptors = catalog(2);
        if large {
            descriptors["snapshots"][1]["path"] = json!("x".repeat(8_000));
        }
        let primary = (0..60)
            .map(|i| json!({"id":i,"sourceOrigin":origin(0)}))
            .collect::<Vec<_>>();
        let peer = json!([{"id":"peer","sourceOrigin":origin(1)}]);
        let row = json!({"data":{"primary":primary,"peer":peer,"sourceEvidence":descriptors}});
        let mut arena = FragmentArena {
            collection_routes: Some(routes()),
            ..Default::default()
        };
        let mut fragments = Vec::new();
        arena
            .plan(row, 12_000, 12_000, &mut fragments)
            .expect("plan");
        assert!(fragments.len() > 1);
        let mut all_primary = Vec::new();
        let mut all_peer = Vec::new();
        for (fragment, planned_chars) in fragments {
            let mut value = arena.materialize(&fragment);
            arena
                .attach_evidence(
                    &fragment,
                    &mut value,
                    &tokio_util::sync::CancellationToken::new(),
                )
                .expect("closure");
            assert!(
                chars(&value) <= 12_000,
                "an atomic peer and a primary row each fit"
            );
            assert!(
                chars(&value) <= planned_chars,
                "closed fragment size is not underestimated"
            );
            exact_closure(&value["data"], &descriptors);
            all_primary.extend(value["data"]["primary"].as_array().unwrap().iter().cloned());
            all_peer.extend(value["data"]["peer"].as_array().unwrap().iter().cloned());
        }
        assert_eq!(all_primary, primary);
        assert_eq!(json!(all_peer), peer);
    }
}
