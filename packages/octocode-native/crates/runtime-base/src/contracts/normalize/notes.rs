//! What a leniently read call says: each reading once, with the rows it was
//! made on.

use super::Note;

/// `path` as a note names it: object keys joined by `.`, a list item by its
/// index in brackets (`resources.a`, `assets[0].source`).
pub(super) fn label(path: &[String]) -> String {
    let mut label = String::new();
    for segment in path {
        if segment.parse::<usize>().is_ok() {
            label.push('[');
            label.push_str(segment);
            label.push(']');
        } else {
            if !label.is_empty() {
                label.push('.');
            }
            label.push_str(segment);
        }
    }
    label
}

/// Each distinct note once, in the order first made, prefixed with the rows
/// it was made on: a batch that sent one form on every row reads one line,
/// not one per row. A note of no row (`None`: the request) reads as made,
/// after the rows' notes.
pub(super) fn per_call(notes: Vec<Note>) -> Vec<String> {
    let mut grouped: Vec<(Option<Vec<usize>>, String)> = Vec::new();
    for (index, note) in notes {
        let same = |(rows, said): &&mut (Option<Vec<usize>>, String)| {
            *said == note && rows.is_some() == index.is_some()
        };
        match (grouped.iter_mut().find(same), index) {
            (Some((Some(rows), _)), Some(index)) if !rows.contains(&index) => rows.push(index),
            (Some(_), _) => {}
            (None, index) => grouped.push((index.map(|index| vec![index]), note)),
        }
    }
    grouped.sort_by_key(|(rows, _)| rows.as_ref().map_or(usize::MAX, |rows| rows[0]));
    grouped
        .into_iter()
        .map(|(rows, note)| {
            let Some(rows) = rows else {
                return note;
            };
            let rows = rows
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",");
            format!("queries[{rows}]: {note}")
        })
        .collect()
}
