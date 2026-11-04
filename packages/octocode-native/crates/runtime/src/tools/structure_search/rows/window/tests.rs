//! A window's rows are the ones building, redacting, ranking and cutting every
//! entry's row would leave, and redaction decides path order.
use super::*;
use crate::tools::test_support::{Fixture, simple_policy};
use octocode_contract_types::{
    SortOrder, StructureSearchSortAnyOrder, StructureSearchSortAnyOrderBy as Key,
    StructureSearchSortAscOnly, StructureSearchSortAscOnlyBy, StructureSearchSortAscOnlyOrder,
};
use octocode_engine::types::FileKind;
use std::time::{Duration, UNIX_EPOCH};

const TOKEN: &str = "ghp_a1B2c3D4a1B2c3D4a1B2c3D4a1B2c3D4a1B2";

/// A walk's entries in walk order: names that redaction moves across others in
/// path order, repeated sizes, times and line counts, every kind.
fn walked(root: &std::path::Path) -> (Vec<FileSystemEntry>, Vec<Option<Lines>>) {
    let relative = [
        "Z.rs".to_owned(),
        "a.rs".to_owned(),
        "a/one.rs".to_owned(),
        format!("a/{TOKEN}.rs"),
        format!("a/{TOKEN}/two.rs"),
        "b".to_owned(),
        "b/three.rs".to_owned(),
        format!("{TOKEN}.rs"),
        TOKEN.to_string(),
        "h.rs".to_owned(),
        "link".to_owned(),
        "pipe".to_owned(),
        "zz.rs".to_owned(),
    ];
    let mut entries = vec![entry(root, "", FileKind::Directory, 0)];
    for (at, path) in relative.iter().enumerate() {
        let kind = match path.as_str() {
            "b" => FileKind::Directory,
            "link" => FileKind::Symlink,
            "pipe" => FileKind::Other,
            token if token == TOKEN => FileKind::Directory,
            _ => FileKind::File,
        };
        entries.push(entry(root, path, kind, at + 1));
    }
    let lines = (0..entries.len())
        .map(|at| (at % 4 != 0).then_some(Lines::Counted(at % 3)))
        .collect();
    (entries, lines)
}

fn entry(root: &std::path::Path, relative: &str, kind: FileKind, at: usize) -> FileSystemEntry {
    let name = relative.rsplit('/').next().unwrap_or(relative).to_owned();
    FileSystemEntry {
        path: if relative.is_empty() {
            root.display().to_string()
        } else {
            format!("{}/{relative}", root.display())
        },
        relative_path: relative.to_owned(),
        name,
        entry_type: kind,
        size: (at % 5 != 4).then_some(i64::try_from(at % 3).expect("small")),
        modified_time: (at % 7 != 6)
            .then(|| UNIX_EPOCH + Duration::from_secs(u64::try_from(at % 4).expect("small"))),
        walk_position: u32::try_from(at).expect("small"),
        stamp: None,
    }
}

/// What the page cut, the digest and the proof read of a row.
fn fields(rows: &[Row]) -> Vec<String> {
    rows.iter()
        .map(|row| {
            format!(
                "{}|{}|{}|{}|{}|{}|{}|{}",
                row.dir,
                row.entry,
                row.display_name,
                row.path,
                row.name,
                row.size,
                row.modified,
                row.source.display()
            )
        })
        .collect()
}

/// The rank every entry's built row took, with its line count.
fn ranked_as_rows(rows: &mut [(Row, Option<Lines>)], sort: &StructureSearchSort) {
    use octocode_contract_types::SortOrder::Desc;
    let StructureSearchSort::AnyOrder(StructureSearchSortAnyOrder { by, order }) = sort else {
        return;
    };
    let directed = |compared: std::cmp::Ordering| {
        if *order == Desc {
            compared.reverse()
        } else {
            compared
        }
    };
    rows.sort_by(|(a, a_lines), (b, b_lines)| match by {
        Key::LineCount => Lines::class(*a_lines)
            .cmp(&Lines::class(*b_lines))
            .then_with(|| directed(Lines::count(*a_lines).cmp(&Lines::count(*b_lines)))),
        Key::SizeBytes => (a.kind != EntryKind::File)
            .cmp(&(b.kind != EntryKind::File))
            .then_with(|| directed(a.size.cmp(&b.size))),
        Key::Name => directed(a.display_name.cmp(&b.display_name)),
        Key::Path => directed(a.path.cmp(&b.path)),
        Key::ModifiedAt => directed(a.modified.total_cmp(&b.modified)),
    });
}

fn sorts() -> Vec<StructureSearchSort> {
    let mut sorts = vec![StructureSearchSort::AscOnly(StructureSearchSortAscOnly {
        by: StructureSearchSortAscOnlyBy::Traversal,
        order: StructureSearchSortAscOnlyOrder::Asc,
    })];
    for by in [
        Key::LineCount,
        Key::SizeBytes,
        Key::Name,
        Key::Path,
        Key::ModifiedAt,
    ] {
        for order in [SortOrder::Asc, SortOrder::Desc] {
            sorts.push(StructureSearchSort::AnyOrder(StructureSearchSortAnyOrder {
                by,
                order,
            }));
        }
    }
    sorts
}

/// Every sort, every window, ties and secret-looking names included: the rows
/// and their order are what ranking every entry's redacted row gives.
#[test]
fn a_window_is_the_cut_of_every_entrys_ranked_row() {
    let root = Fixture::new();
    let (_, security) = simple_policy(&root.0);
    let (entries, lines) = walked(&root.0);
    for sort in sorts() {
        let mut every = entries
            .iter()
            .zip(&lines)
            .filter(|(entry, _)| !Row::is_root_dir(entry, &root.0))
            .map(|(entry, lines)| {
                let row = Row::of(entry, Row::listed_path(entry, &root.0, &security), *lines);
                (row, *lines)
            })
            .collect::<Vec<_>>();
        ranked_as_rows(&mut every, &sort);
        let every = fields(&every.into_iter().map(|(row, _)| row).collect::<Vec<_>>());
        assert_eq!(every.len(), 13);
        for start in 0..=every.len() + 1 {
            for len in [1, 2, 5, every.len() + 2] {
                let (rows, available) = window_rows(
                    &entries,
                    &lines,
                    (&root.0, &security),
                    &sort,
                    start..start + len,
                );
                assert_eq!(available, every.len());
                let cut = every[start.min(every.len())..(start + len).min(every.len())].to_vec();
                assert_eq!(fields(&rows), cut, "{sort:?}, {start}+{len}");
            }
        }
    }
}

/// Path order ranks by the path delivered, so what a token sorts as stays
/// unseen in the order: a window that ranked raw names would answer otherwise.
#[test]
fn path_order_ranks_by_the_redacted_path() {
    let root = Fixture::new();
    let (_, security) = simple_policy(&root.0);
    let (entries, lines) = walked(&root.0);
    let by_path = sorts()
        .into_iter()
        .find(|sort| {
            matches!(sort, StructureSearchSort::AnyOrder(any)
                if any.by == Key::Path && any.order == SortOrder::Asc)
        })
        .expect("path order");
    let (rows, _) = window_rows(&entries, &lines, (&root.0, &security), &by_path, 0..13);
    let paths = rows.iter().map(|row| row.path.as_str()).collect::<Vec<_>>();
    assert!(paths.iter().all(|path| !path.contains(TOKEN)), "{paths:?}");
    let sorted = {
        let mut sorted = paths.clone();
        sorted.sort_unstable();
        sorted
    };
    assert_eq!(paths, sorted);
    let raw_rank = |name: &str| {
        let mut raw = entries
            .iter()
            .skip(1)
            .map(|entry| entry.relative_path.as_str())
            .collect::<Vec<_>>();
        raw.sort_unstable();
        raw.iter().position(|path| *path == name).expect("listed")
    };
    let delivered_rank = |name: &str| paths.iter().position(|path| *path == name).expect("listed");
    assert_ne!(
        raw_rank("a.rs"),
        delivered_rank("a.rs"),
        "the fixture must tell the two orders apart"
    );
}

/// Name order ranks by the name delivered, so where a token's raw name falls
/// among its neighbors stays unseen.
#[test]
fn name_order_ranks_by_the_delivered_name() {
    let root = Fixture::new();
    let (_, security) = simple_policy(&root.0);
    let names = [
        "a.txt",
        "empty.txt",
        &format!("{TOKEN}.txt"),
        "ghq.txt",
        "zz.txt",
    ];
    let entries = names
        .iter()
        .enumerate()
        .map(|(at, name)| entry(&root.0, name, FileKind::File, at + 1))
        .collect::<Vec<_>>();
    let lines = vec![None; entries.len()];
    let by_name = |order| {
        StructureSearchSort::AnyOrder(StructureSearchSortAnyOrder {
            by: Key::Name,
            order,
        })
    };
    for order in [SortOrder::Asc, SortOrder::Desc] {
        let (rows, _) = window_rows(
            &entries,
            &lines,
            (&root.0, &security),
            &by_name(order),
            0..names.len(),
        );
        let shown = rows
            .iter()
            .map(|row| row.display_name.as_str())
            .collect::<Vec<_>>();
        assert!(shown.iter().all(|name| !name.contains(TOKEN)), "{shown:?}");
        let mut sorted = shown.clone();
        sorted.sort_unstable();
        if order == SortOrder::Desc {
            sorted.reverse();
        }
        assert_eq!(shown, sorted);
    }
}
