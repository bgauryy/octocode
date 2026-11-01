use super::*;

/// The banner names the query indexes later pages serve or continue; how
/// to follow `next` lives once in the instructions.
#[test]
fn the_incomplete_banner_names_the_queries_later_pages_hold() {
    let banner = |unserved: &[usize], continued: &[usize]| {
        let mut structured = Map::new();
        lead_with_incomplete_banner(
            &mut structured,
            &Later {
                unserved: unserved.to_vec(),
                continued: continued.to_vec(),
            },
        );
        structured["warnings"].clone()
    };
    assert_eq!(
        banner(&[1, 2], &[0]),
        json!(["incomplete: later response pages serve queries 1, 2 and continue query 0"])
    );
    assert_eq!(
        banner(&[3], &[]),
        json!(["incomplete: later response pages serve query 3"])
    );
    assert_eq!(
        banner(&[], &[4]),
        json!(["incomplete: later response pages continue query 4"])
    );
}

/// The room the plan keeps for the banner holds every banner a page of
/// those rows can carry.
#[test]
fn the_banner_room_holds_the_widest_banner() {
    let skeleton = Map::new();
    let queries = [0, 7, 12, 130];
    let room = incomplete_banner_chars(&skeleton, &queries);
    for split in 0..=queries.len() {
        let (unserved, continued) = queries.split_at(split);
        let mut structured = Map::new();
        lead_with_incomplete_banner(
            &mut structured,
            &Later {
                unserved: unserved.to_vec(),
                continued: continued.to_vec(),
            },
        );
        let chars = octocode_engine::json_size::utf16_units(&structured).unwrap_or(0) - 2;
        assert!(chars <= room, "{chars} > {room}: {structured:?}");
    }
}
