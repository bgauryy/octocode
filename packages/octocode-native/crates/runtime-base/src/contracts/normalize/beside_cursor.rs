//! A cursor call that also sends query fields, read against the saved query
//! its cursor restores. The cursor runs that whole query, so a field sent
//! beside it at the value the query already holds has one reading and no
//! effect: it is dropped and said, as a page size beside a cursor is
//! ([`drop_lone_page_size`]). A field the saved query holds at another value,
//! or does not hold, asks for another query: the call is left for validation
//! to refuse, led by the fields that refuse it, and the refusal can name what
//! the cursor holds. A private progress field
//! ([`crate::contracts::state_fields`]) is never compared: only the issued
//! cursor supplies one.

use serde_json::{Map, Value};

use super::PAGE_SIZE;
use crate::id::ToolId;

/// What the fields beside a cursor are, read against its saved query.
#[derive(Debug, PartialEq, Eq)]
pub enum BesideCursor {
    /// Nothing more to say: the cursor alone, or a call validation refuses
    /// in its own words, led by the fields that refuse it (a private field,
    /// a request field, an unknown one, one the saved query does not hold).
    AsSent,
    /// Every query field beside the cursor held the value its saved query
    /// holds: they were dropped, and these notes say so.
    Held(Vec<String>),
    /// A field beside the cursor refuses the call: the call now leads with
    /// the fields that refuse it, and those at the values the saved query
    /// holds were dropped. This sentence names the saved values of the fields
    /// that refuse it, and the dropped fields as ones to drop, for the
    /// refusal.
    Holds(String),
}

/// `input`, a `tool_name` call holding a cursor, with the query fields
/// beside the cursor dropped when each holds the value every row of `saved`
/// (the request that cursor restores) holds. A page size beside them is then
/// dropped as [`drop_lone_page_size`] drops it. A call any other field
/// refuses leads with the fields that refuse it, as validation names a
/// request's fields in the order it holds them; beside them, the fields at
/// their saved values are dropped, unless a private field stopped the
/// comparison. Any other call is left as sent.
pub fn read_beside_cursor(tool_name: &str, input: &mut Value, saved: &Value) -> BesideCursor {
    let Some(id) = ToolId::from_name(tool_name) else {
        return BesideCursor::AsSent;
    };
    let Some(request) = input.as_object_mut() else {
        return BesideCursor::AsSent;
    };
    if !request.get("cursor").is_some_and(Value::is_string) {
        return BesideCursor::AsSent;
    }
    let beside: Vec<String> = request
        .keys()
        .filter(|field| *field != "cursor" && *field != PAGE_SIZE)
        .cloned()
        .collect();
    if beside.is_empty() {
        return match drop_lone_page_size(request, Some(saved)) {
            PageSize::Absent => BesideCursor::AsSent,
            PageSize::AtIssued => BesideCursor::Held(Vec::new()),
            PageSize::Said(note) => BesideCursor::Held(vec![note]),
        };
    }
    // Private progress is the issued cursor's alone: a caller's is refused
    // before any value is read, and leads the refusal.
    let private = crate::contracts::state_fields(id);
    let sent_private: Vec<&String> = beside
        .iter()
        .filter(|field| private.contains(*field))
        .collect();
    if !sent_private.is_empty() {
        lead_with(request, &sent_private);
        return BesideCursor::AsSent;
    }
    // A field no query form declares (a request field, an unknown one), or
    // one the saved query does not hold at the value sent, refuses the call.
    let (refusing, at_saved): (Vec<&String>, Vec<&String>) = beside.iter().partition(|field| {
        !crate::contracts::query_declares(id, field)
            || request.get(field.as_str()) != held(saved, field)
    });
    if !refusing.is_empty() {
        let holds: Vec<String> = refusing
            .iter()
            .filter_map(|field| {
                held(saved, field).map(|value| format!("`{field}: {}`", super::sketch(value)))
            })
            .collect();
        for field in &at_saved {
            request.shift_remove(field.as_str());
        }
        lead_with(request, &refusing);
        let mut said = Vec::new();
        if !holds.is_empty() {
            said.push(format!("The cursor's query holds {}.", holds.join(", ")));
        }
        if !at_saved.is_empty() {
            let (is, values) = if at_saved.len() == 1 {
                ("is", "the value")
            } else {
                ("are", "the values")
            };
            said.push(format!(
                "{} {is} at {values} the cursor's query already holds and can be dropped.",
                backticked(&at_saved)
            ));
        }
        return if said.is_empty() {
            BesideCursor::AsSent
        } else {
            BesideCursor::Holds(said.join(" "))
        };
    }
    for field in &beside {
        request.shift_remove(field);
    }
    let named = backticked(&beside);
    let (was, them) = if beside.len() == 1 {
        ("was", "it")
    } else {
        ("were", "them")
    };
    let mut notes = vec![format!(
        "{named} beside `cursor` {was} not applied: the cursor's query already holds {them}; send {{\"cursor\":\"…\"}} alone."
    )];
    if let PageSize::Said(note) = drop_lone_page_size(request, Some(saved)) {
        notes.push(note);
    }
    BesideCursor::Held(notes)
}

/// A cursor whose walk could not be opened, sent with a page size and nothing
/// else: the cursor alone, with the note that says so. The refusal that
/// follows is the cursor's own.
pub fn drop_unopened_page_size(input: &mut Value) -> Option<String> {
    match drop_lone_page_size(input.as_object_mut()?, None) {
        PageSize::Said(note) => Some(note),
        PageSize::Absent | PageSize::AtIssued => None,
    }
}

/// A page size sent beside a cursor and nothing else, dropped: the cursor
/// names a page of a walk cut at the size it was issued with (another size
/// would cut other pages, so the same cursor would skip or repeat rows), so
/// a size has one reading, no effect. [`PageSize::Absent`]: nothing to drop. [`PageSize::AtIssued`]:
/// the size is the one the walk was cut at (`saved`'s, else the default), so
/// dropping it needs no note. [`PageSize::Said`]: another size, dropped and
/// said.
fn drop_lone_page_size(request: &mut Map<String, Value>, saved: Option<&Value>) -> PageSize {
    if request.len() != 2 || !request.contains_key("cursor") {
        return PageSize::Absent;
    }
    let Some(sent) = request.shift_remove(PAGE_SIZE) else {
        return PageSize::Absent;
    };
    let issued = saved.map(|saved| {
        saved
            .get(PAGE_SIZE)
            .and_then(chars)
            .unwrap_or(crate::id::DEFAULT_RESPONSE_CHARS as u64)
    });
    if issued.is_some() && chars(&sent) == issued {
        return PageSize::AtIssued;
    }
    PageSize::Said(format!(
        "`{PAGE_SIZE}` beside `cursor` was not applied: a cursor keeps the page size it was issued with; send {{\"cursor\":\"…\"}} alone, and `{PAGE_SIZE}` with queries in a new request."
    ))
}

/// What a lone page size beside a cursor came to.
enum PageSize {
    /// None was sent: nothing dropped.
    Absent,
    /// Dropped; it is the size the walk was cut at.
    AtIssued,
    /// Dropped; another size, with the note that says so.
    Said(String),
}

/// A page size as a number, sent as one or as its digits.
fn chars(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
}

/// `fields` as the notes name them: `` `a`, `b` ``.
fn backticked(fields: &[impl AsRef<str>]) -> String {
    fields
        .iter()
        .map(|field| format!("`{}`", field.as_ref()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// `request` with `fields` first, in their order, then the rest as sent:
/// validation names a request's fields in the order it holds them, so its
/// refusal leads with them.
fn lead_with(request: &mut Map<String, Value>, fields: &[impl AsRef<str>]) {
    let mut rest = std::mem::take(request);
    for field in fields {
        if let Some((field, value)) = rest.shift_remove_entry(field.as_ref()) {
            request.insert(field, value);
        }
    }
    request.append(&mut rest);
}

/// The one value every row of the saved request holds at `field`; none when
/// a row lacks it or the rows disagree.
fn held<'a>(saved: &'a Value, field: &str) -> Option<&'a Value> {
    let mut rows = saved.get("queries")?.as_array()?.iter();
    let value = rows.next()?.get(field)?;
    rows.all(|row| row.get(field) == Some(value))
        .then_some(value)
}
