//! What a repaired call says: one note per field it read in another name or
//! form, each naming the form sent and the form it ran, which is the form to
//! send on a row that runs, and its other repairs (a request's shape, a value
//! left out) as they were made.

use serde_json::Value;

/// One repair of a field: what was sent, and the fields the call runs in its
/// place, said with the values it runs them with: "read `{sent}` as
/// `{ran}`; send `{ran}` next time.", or only what was read on a row that
/// still fails, whose form the contract does not take.
pub(super) struct Repair {
    /// The row repaired; `None`: a request field.
    row: Option<usize>,
    /// The fields the call runs, by path below the row (or the request): the
    /// one repaired first, then any set beside it (`regexEngine` beside
    /// `matchString`).
    field: Vec<Vec<String>>,
    /// What was sent, as the note quotes it: a field with its value
    /// (`limit: "5"`), or the name of one read as another field (`pattern`).
    sent: Vec<String>,
    /// The value sent: a field that runs with it is said by its name alone
    /// (`read `depth` as `maxDepth``).
    value: Option<Value>,
    /// Whether `sent` names fields read as others, which a note on the same
    /// field also names; a value read in another form is said by that note's
    /// own value.
    renamed: bool,
}

impl Repair {
    /// `sent` at `at` read in another form or as another value, which the
    /// call runs at `field`: `at` itself, or the field a spelling reads it as
    /// (`filters.state: "merged"` as `filters.isMerged`).
    pub(super) fn value(
        row: Option<usize>,
        at: &[String],
        sent: &Value,
        field: Vec<String>,
    ) -> Self {
        Self {
            row,
            field: vec![field],
            sent: vec![format!("{}: {}", label(at), super::sketch(sent))],
            value: Some(sent.clone()),
            renamed: false,
        }
    }

    /// The fields named `sent` read as `field`; `value` is the one value they
    /// sent, when it is one.
    pub(super) fn renamed(
        row: Option<usize>,
        sent: Vec<String>,
        field: Vec<Vec<String>>,
        value: Option<Value>,
    ) -> Self {
        Self {
            row,
            field,
            sent,
            value,
            renamed: true,
        }
    }

    /// The note, given the row (or the request) as the call runs it, and
    /// whether that row runs: only then is the form it ran one to send.
    fn said(&self, ran: &Value, runs_row: bool) -> String {
        let runs = self
            .field
            .iter()
            .map(|path| {
                let label = label(path);
                match ran.pointer(&crate::json_pointer::of(path)) {
                    Some(runs) if Some(runs) != self.value.as_ref() => {
                        format!("`{label}: {}`", super::sketch(runs))
                    }
                    _ => format!("`{label}`"),
                }
            })
            .collect::<Vec<_>>()
            .join(" with ");
        let sent = self
            .sent
            .iter()
            .map(|sent| format!("`{sent}`"))
            .collect::<Vec<_>>()
            .join(", ");
        if runs_row {
            format!("read {sent} as {runs}; send {runs} next time.")
        } else {
            format!("read {sent} as {runs}.")
        }
    }

    /// Whether this repair holds `other`'s field: at or above it, so its
    /// note, said with the whole value the call runs, also says `other`'s.
    fn holds(&self, other: &Self) -> bool {
        self.row == other.row
            && other.field.first().is_some_and(|inner| {
                self.field
                    .iter()
                    .any(|outer| inner.starts_with(outer.as_slice()))
            })
    }
}

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

/// What the call says of one repair.
pub(super) enum Note {
    /// A field read in another name or form: said by [`Repair::said`], once
    /// per field.
    Field(Repair),
    /// Any other repair, said as made, of its row (`None`: the request).
    Said(Option<usize>, String),
}

impl Note {
    fn row(&self) -> Option<usize> {
        match self {
            Self::Field(repair) => repair.row,
            Self::Said(row, _) => *row,
        }
    }
}

/// Each note's text with its row, `ran` being the request as it runs: one
/// note per repaired field of a row. A repair of a field another repair of
/// the row holds (`sort.by` inside a completed `sort`), or of the field an
/// earlier repair wrote, is said by that note, which is said with the whole
/// value the field runs with; the names a rename sent are added to a rename
/// that holds it (`count` and `countOnly` both read as `view`). A repair on
/// a row of `failing`, which still fails as it runs, says only what was read.
pub(super) fn said(
    notes: Vec<Note>,
    ran: &Value,
    failing: &[usize],
) -> Vec<(Option<usize>, String)> {
    let mut kept: Vec<Option<Note>> = Vec::with_capacity(notes.len());
    for note in notes {
        let Note::Field(mut repair) = note else {
            kept.push(Some(note));
            continue;
        };
        let holder = kept.iter_mut().flatten().find_map(|note| match note {
            Note::Field(earlier) if earlier.holds(&repair) => Some(earlier),
            _ => None,
        });
        if let Some(holder) = holder {
            absorb(holder, repair);
            continue;
        }
        for slot in &mut kept {
            let held = matches!(slot, Some(Note::Field(inner)) if repair.holds(inner));
            if held && let Some(Note::Field(inner)) = slot.take() {
                absorb_inner(&mut repair, inner);
            }
        }
        kept.push(Some(Note::Field(repair)));
    }
    kept.into_iter()
        .flatten()
        .map(|note| {
            let row = note.row();
            let text = match note {
                Note::Said(_, text) => text,
                Note::Field(repair) => {
                    let at = match repair.row {
                        Some(index) => &ran["queries"][index],
                        None => ran,
                    };
                    let runs_row = !repair.row.is_some_and(|row| failing.contains(&row));
                    repair.said(at, runs_row)
                }
            };
            (row, text)
        })
        .collect()
}

/// `repair`, held by `holder`, said by it: the names a rename sent join a
/// rename's note, after its own.
fn absorb(holder: &mut Repair, repair: Repair) {
    if holder.renamed && repair.renamed {
        for sent in repair.sent {
            if !holder.sent.contains(&sent) {
                holder.sent.push(sent);
            }
        }
        holder.value = None;
    }
}

/// `inner`, made before the `outer` repair that holds it, said by `outer`:
/// the names a rename sent come first, as they were made.
fn absorb_inner(outer: &mut Repair, inner: Repair) {
    if outer.renamed && inner.renamed {
        let mut sent = inner.sent;
        for name in std::mem::take(&mut outer.sent) {
            if !sent.contains(&name) {
                sent.push(name);
            }
        }
        outer.sent = sent;
        outer.value = None;
    }
}

/// Each distinct repair note once, in the order first made, prefixed with
/// the rows it was made on: a batch that sent one guess on every row reads
/// one line, not one per row. A note of no row (`None`: a request field)
/// reads as made.
pub(super) fn per_call(notes: Vec<Note>, ran: &Value, failing: &[usize]) -> Vec<String> {
    let mut grouped: Vec<(Option<Vec<usize>>, String)> = Vec::new();
    for (index, note) in said(notes, ran, failing) {
        let same = |(rows, said): &&mut (Option<Vec<usize>>, String)| {
            *said == note && rows.is_some() == index.is_some()
        };
        match (grouped.iter_mut().find(same), index) {
            (Some((Some(rows), _)), Some(index)) if !rows.contains(&index) => rows.push(index),
            (Some(_), _) => {}
            (None, index) => grouped.push((index.map(|index| vec![index]), note)),
        }
    }
    // In row order, a request field's notes last; a row's in the order made.
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
