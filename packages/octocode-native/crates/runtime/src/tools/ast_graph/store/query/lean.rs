//! A row's default view: file, line and what the edge says. Byte spans, digests
//! and occurrence identities of source that is plainly the file's own text
//! return with `includeProvenance:true`. A row with an origin that is not plain
//! text keeps all of its provenance, and a parse that was not clean stays,
//! because they qualify the row.
use serde_json::{Map, Value};

/// Each span and the origin that places it.
const SPANS: [(&str, &str); 3] = [
    ("sourceRange", "sourceOrigin"),
    ("selectionByteRange", "selectionOrigin"),
    ("bindingSourceRange", "bindingOrigin"),
];

/// Objects that are evidence whole, never rows of their own.
const OWN_EVIDENCE: [&str; 5] = [
    "sourceEvidence",
    "sourceOrigin",
    "selectionOrigin",
    "bindingOrigin",
    "recipient",
];

pub(super) fn project(value: &mut Value) {
    match value {
        Value::Array(rows) => rows.iter_mut().for_each(project),
        Value::Object(row) => project_row(row),
        _ => {}
    }
}

/// Whether every origin on the row, its call recipient's included, is plain.
fn plain_origins(row: &Map<String, Value>) -> bool {
    let plain = |origin: Option<&Value>| {
        origin.is_none_or(|origin| origin.is_null() || origin == "identity")
    };
    SPANS.iter().all(|(_, origin)| plain(row.get(*origin)))
        && plain(
            row.get("recipient")
                .and_then(|recipient| recipient.get("sourceOrigin")),
        )
}

fn project_row(row: &mut Map<String, Value>) {
    if plain_origins(row) {
        for (span, origin) in SPANS {
            row.remove(span);
            row.remove(origin);
        }
        row.remove("sourceDigest");
        row.remove("occurrenceId");
        row.remove("recipient");
        if row
            .get("parseAssessment")
            .is_some_and(|assessment| assessment["status"] == "parsed")
        {
            row.remove("parseAssessment");
        }
    }
    for (key, value) in row.iter_mut() {
        if !OWN_EVIDENCE.contains(&key.as_str()) {
            project(value);
        }
    }
}
