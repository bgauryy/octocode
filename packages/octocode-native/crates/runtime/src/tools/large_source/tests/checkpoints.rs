//! The every-value fallback keeps checkpoints, never one edit per value: its
//! state follows the source's size, a read it has no memory for is refused whole,
//! and every page and byte page shows the whole-file masked view.
use super::super::edit_map::{EditMap, build};
use super::super::error::ReadError;
use super::super::sealed::{Edit, keyed_offset, merge, outside_key_lines, shift};
use super::masks::every_value;
use super::*;
use octocode_engine::security::{Named, Objects, every_value_spans};
use octocode_io::memo::MemoryBudget;
use std::sync::Arc;

fn roomy() -> &'static MemoryBudget {
    crate::tools::page_memo::test_budget!(256 * 1024 * 1024)
}
fn idle() -> &'static MemoryBudget {
    crate::tools::page_memo::test_budget!(256 * 1024 * 1024)
}
/// A budget just above what one block of [`members`] with 1-byte values needs:
/// its bytes (a doubling vector, old and new block at once), its spans and the
/// worst case of its edits, each at the code's own sizes.
fn one_block_budget() -> &'static MemoryBudget {
    crate::tools::page_memo::test_budget!(one_block())
}
fn tight() -> &'static MemoryBudget {
    crate::tools::page_memo::test_budget!(64 * 1024)
}

fn one_block() -> usize {
    let interval = usize::try_from(super::super::proven::CHECKPOINT_BYTES).expect("interval");
    let values = interval / member(1).len() + 1;
    let edit = std::mem::size_of::<Edit>()
        + octocode_engine::security::redaction_placeholder(NAMED.objects.form().pattern()).len();
    4 * interval
        + 3 * values * std::mem::size_of::<octocode_engine::security::SecretSpan>()
        + 3 * values * edit
}

const NAMED: Named = Named {
    objects: Objects::ALL,
    yaml: false,
};

/// Holds `budget` for this thread's reads until it drops.
struct Held;

impl Held {
    fn to(budget: &'static MemoryBudget) -> Self {
        super::super::set_memory(Some(budget));
        Self
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        super::super::set_memory(None);
    }
}

/// One JSON string member, its value `value` bytes long.
fn member(value: usize) -> String {
    format!("\"k\":\"{}\",", "v".repeat(value))
}

/// `bytes` of JSON string members, each value `value` bytes long.
fn members(bytes: usize, value: usize) -> String {
    let member = member(value);
    format!("{{\"items\":[{}0]}}\n", member.repeat(bytes / member.len()))
}

/// The index of `text`, as a block map over a file of it.
fn indexed(
    file: &tempfile::NamedTempFile,
    text: &str,
    key_lines: Vec<(std::ops::Range<usize>, usize)>,
) -> Result<EditMap, ReadError> {
    let digest = hex::encode(Sha256::digest(text.as_bytes()));
    build(
        &opened(file.path()),
        NAMED,
        &digest,
        Arc::from(Vec::<Edit>::new()),
        key_lines.into(),
        &NeverCancel,
    )
    .map(|index| EditMap::Blocks(Arc::new(index)))
}

/// What the checkpoints hold follows the source's size over the interval, not
/// how many values it holds.
#[test]
fn the_held_state_follows_size_over_interval_not_values() {
    let _idle = Held::to(idle());
    let size = 12 * 1024 * 1024;
    let interval = usize::try_from(super::super::proven::CHECKPOINT_BYTES).expect("interval");
    let (dense, sparse) = (members(size, 1), members(size, 1000));
    let (dense_file, sparse_file) = (temp(dense.as_bytes()), temp(sparse.as_bytes()));
    let dense_map = indexed(&dense_file, &dense, Vec::new()).expect("dense");
    let sparse_map = indexed(&sparse_file, &sparse, Vec::new()).expect("sparse");
    let (dense_points, sparse_points) = (
        dense_map.checkpoints().expect("blocks"),
        sparse_map.checkpoints().expect("blocks"),
    );
    let values = every_value_spans(dense.as_bytes(), NAMED).len();
    assert!(dense_points <= dense.len() / interval + 1, "{dense_points}");
    assert!(dense_points * interval / 2 < dense.len(), "{dense_points}");
    assert!(dense_points.abs_diff(sparse_points) <= 1);
    assert!(values > 1000 * dense_points, "{values} values");
    assert_eq!(idle().used(), 0, "nothing stays held beyond the map");
}

/// A budget with no room for a block refuses the pass; it never serves less.
#[test]
fn a_budget_without_room_for_a_block_refuses_the_index() {
    let _tight = Held::to(tight());
    let text = members(3 * 1024 * 1024, 1);
    let file = temp(text.as_bytes());
    assert_eq!(
        indexed(&file, &text, Vec::new()).err(),
        Some(ReadError::Refused)
    );
    assert_eq!(tight().used(), 0);
}

/// The blocks' edits, shifts and keyed offsets are the whole-file list's.
#[test]
fn blocks_place_every_offset_as_the_whole_file_list_does() {
    let _roomy = Held::to(roomy());
    let line = "{\"data\":{\"k\":\"v000001\"},\"kind\":\"Secret\"},\n";
    let text = line.repeat(3_700_000 / line.len());
    let keys = vec![(line.len() * 30_000..line.len() * 30_001, 40usize)];
    let file = temp(text.as_bytes());
    let map = indexed(&file, &text, keys.clone()).expect("index");
    assert!(map.checkpoints().expect("blocks") > 3);
    let handle = opened(file.path());
    let expected = outside_key_lines(
        merge(
            Vec::new(),
            every_value_spans(text.as_bytes(), NAMED)
                .iter()
                .fold(Vec::new(), |mut edits, span| {
                    let mut lines = super::super::masks::PlaceholderEdits::new(span);
                    lines.feed(&text.as_bytes()[span.range.clone()], &mut edits);
                    lines.finish(&mut edits);
                    edits
                }),
        ),
        &keys,
    );
    assert!(map.all(&handle).expect("all") == expected, "edits differ");
    let end = text.len();
    let step = end / 37;
    for (from, to) in (0..end).step_by(step).flat_map(|from| {
        [end, from + step / 3, from + 1_048_577, from + 2_300_000]
            .into_iter()
            .map(move |to| (from, to.min(end)))
    }) {
        assert_eq!(
            map.shift(&handle, from..to).expect("shift"),
            shift(&expected, from..to),
            "{from}..{to}"
        );
    }
    for start in (0..text.len() / line.len())
        .step_by(9_999)
        .map(|n| n * line.len())
    {
        for sealed in [0, 1, 17, 90_000, 1_500_000, 3_000_000, 9_000_000] {
            assert_eq!(
                map.keyed(&handle, &keys, start, sealed).expect("keyed"),
                keyed_offset(&expected, &keys, start, sealed),
                "{start} + {sealed}"
            );
        }
    }
}

/// A line of Secret objects past the checkpoint interval.
fn long_secret_line() -> String {
    let mut line = String::from(r#"{"items":["#);
    let mut index = 0;
    while line.len() < 3 * 1024 * 1024 {
        line.push_str(&format!(
            r#"{{"note":{{"k":"v{index:06}"}},"kind":"Secret"}},"#
        ));
        index += 1;
    }
    line.push_str("{\"kind\":\"ConfigMap\"}]}\n");
    line
}

/// Byte pages of a few KiB walk a line across many checkpoints and rejoin the
/// whole-file view exactly, windows ending inside values.
#[test]
fn a_tiny_window_byte_walk_rejoins_the_whole_file_view() {
    let line = long_secret_line();
    let file = temp(line.as_bytes());
    // The scanner redacts nothing of this line: its view is every value hidden.
    let expected = every_value(&line, true, false);
    let handle = opened(file.path());
    super::super::set_walk_budget(Some(1));
    let (mut shown, mut from, mut pages) = (Vec::new(), 0usize, 0);
    loop {
        let want = Want {
            first: 1,
            last: 1,
            from,
            room: 65_537,
        };
        let window = stream(&handle, &[want], &NeverCancel);
        let Ok(mut windows) = window else {
            super::super::set_walk_budget(None);
            panic!("page {pages} at {from}: {:?}", window.err());
        };
        let page = windows.remove(0);
        assert!(page.every_value);
        if page.window.is_empty() {
            break;
        }
        from += page.window.len();
        shown.extend_from_slice(&page.window);
        pages += 1;
        assert!(pages < 10_000, "the walk ends");
    }
    super::super::set_walk_budget(None);
    assert!(pages > 4, "{pages} pages");
    let differs = shown
        .iter()
        .zip(expected.as_bytes())
        .position(|(shown, view)| shown != view);
    assert!(
        shown == expected.as_bytes(),
        "the pages rejoin the view: {} of {} bytes, {pages} pages, first difference {differs:?}: {:?} against {:?}",
        shown.len(),
        expected.len(),
        String::from_utf8_lossy(
            &shown[differs.unwrap_or(0).saturating_sub(120)
                ..(differs.unwrap_or(0) + 200).min(shown.len())]
        ),
        &expected[differs.unwrap_or(0).saturating_sub(120)
            ..(differs.unwrap_or(0) + 200).min(expected.len())]
    );
}

/// A read the budget has no room to place the values of serves nothing, and
/// says why.
#[test]
fn a_read_over_the_memory_budget_is_refused_whole() {
    let line = long_secret_line();
    let file = temp(line.as_bytes());
    let _tight = Held::to(tight());
    super::super::set_walk_budget(Some(1));
    let page = fetch(
        &file,
        &query(serde_json::json!({"path":file.path(),"ranges":[{"startLine":1,"endLine":1}]})),
        None,
    );
    super::super::set_walk_budget(None);
    assert_eq!(page.status, ReadStatus::Error, "{page:?}");
    assert!(page.content.is_none(), "no partial view");
    assert!(
        page.partial_reasons
            .contains(&PartialReason::SecurityWalkMemoryBudget),
        "{page:?}"
    );
    assert_eq!(page.terminal_limit, Some(true));
    assert!(!format!("{page:?}").contains("v0000"));
}

/// What one block needs is all the budget a source of any length needs: the
/// bytes and spans of a block that closed are given back.
#[test]
fn a_budget_that_fits_one_block_indexes_a_source_of_many() {
    let _one = Held::to(one_block_budget());
    let text = members(16 * 1024 * 1024, 1);
    let file = temp(text.as_bytes());
    let map = indexed(&file, &text, Vec::new()).expect("a block fits");
    assert!(map.checkpoints().expect("blocks") > 8);
    assert_eq!(one_block_budget().used(), 0);
}

/// Several ranges of a read the budget has no room for are refused whole too,
/// with the same reason as a single window.
#[test]
fn ranges_over_the_memory_budget_are_refused_with_the_reason() {
    let text = format!("{{\"a\":1}}\n{}{{\"c\":3}}\n", long_secret_line());
    let file = temp(text.as_bytes());
    let _tight = Held::to(tight());
    super::super::set_walk_budget(Some(1));
    let page = fetch(
        &file,
        &query(serde_json::json!({"path":file.path(),"ranges":[
            {"startLine":1,"endLine":1},{"startLine":3,"endLine":3}]})),
        None,
    );
    super::super::set_walk_budget(None);
    assert_eq!(page.status, ReadStatus::Error, "{page:?}");
    assert!(page.content.is_none(), "no partial view");
    assert!(
        page.partial_reasons
            .contains(&PartialReason::SecurityWalkMemoryBudget),
        "{page:?}"
    );
    assert_eq!(page.terminal_limit, Some(true));
}
