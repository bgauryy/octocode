use serde::Serialize;

/// Characters that give a text a regex reading besides its literal one.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SearchStatus {
    #[default]
    Success,
    Empty,
    Partial,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchMatch {
    pub line: u32,
    /// Start of the hit on its line. Span rows (`matchOnly`) carry it, since
    /// several spans can share a line, and so does a window row
    /// (`window_start`), whose value is part of its line. A whole-line row's
    /// value shows it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    pub value: String,
    /// Byte of its line the value starts at, when the value is a window
    /// around the hit of a line too long for a row and not the line whole
    /// (see [`octocode_engine::types::RowWindow`]). Read to keep the row's
    /// `column` and to hand the line to a read, never shown.
    #[serde(skip)]
    pub window_start: Option<u32>,
    /// Byte offset of the hit in its line: where a byte-window read of a long
    /// line starts at the match. Read by leads, never shown.
    #[serde(skip)]
    pub byte_offset: u32,
    /// Source line the value starts at, as the search joined it: its match
    /// line less the context lines before it, which stop at an absent
    /// neighbour (a line `testCode` left out). Read to number a window,
    /// never shown.
    #[serde(skip)]
    pub first_line: u32,
    /// The last line of a match spanning several (multiline mode): the
    /// match covers `line..=endLine`; absent for a one-line match.
    #[serde(rename = "endLine", skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    /// Every hit line inside a merged context block, in order; present
    /// only when overlapping/adjacent windows were merged into this one.
    #[serde(rename = "matchedLines", skip_serializing_if = "Option::is_none")]
    pub match_lines: Option<Vec<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    /// Index in the file's `declarations` of the declaration around this hit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enclosing: Option<u32>,
    /// `true` on a row whose hit line declares a name (the lspSearch
    /// anchor among same-name hits); absent on every other row.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declaration: Option<bool>,
}

/// A `pagination` block: one page's facts in the one pagination shape
/// ([`crate::response::pages::PageFacts`]), and whether units are left
/// after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Paging {
    facts: crate::response::pages::PageFacts,
    pub(crate) has_more: bool,
    /// `total` is a floor: the walk goes on past the rows counted.
    loaded: bool,
}

impl Paging {
    /// The page of `length` `unit`s from `offset` of `total`; `past_end`
    /// when it was requested past the end.
    pub(crate) fn of(
        unit: &'static str,
        (offset, length, total): (usize, usize, usize),
        has_more: bool,
        past_end: bool,
    ) -> Self {
        Self {
            facts: crate::response::pages::PageFacts::open(offset, length, has_more)
                .with_unit(unit)
                .with_total(total)
                .out_of_range(past_end),
            has_more,
            loaded: false,
        }
    }

    /// `total` counts only what the walk has shown so far (`countScope`
    /// `loaded`) while `floor`.
    pub(crate) fn floor(mut self, floor: bool) -> Self {
        self.loaded = floor;
        self
    }
}

impl Serialize for Paging {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut value = self.facts.to_value();
        if self.loaded {
            value["countScope"] = serde_json::json!("loaded");
        }
        value.serialize(serializer)
    }
}

/// A declaration enclosing a hit, named like the inputs that take it
/// (`symbolName` + `line` = `line`, `ranges` = `line-endLine`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Enclosing {
    pub symbol_name: String,
    pub kind: String,
    pub line: u32,
    pub end_line: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFile {
    pub path: String,
    /// JSON and TOML files: each hit line's key path (`matchKeys`).
    #[serde(
        rename = "matchKeys",
        skip_serializing_if = "Vec::is_empty",
        serialize_with = "crate::tools::match_keys::serialize_rows"
    )]
    pub match_keys: Vec<(usize, String)>,
    /// The declarations around the shown hits, each once; a hit names its own
    /// by index, so hits in one function do not each repeat it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub declarations: Vec<Enclosing>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matches: Option<Vec<SearchMatch>>,
    #[serde(rename = "matchCount", skip_serializing_if = "Option::is_none")]
    pub total_occurrences: Option<u32>,
    #[serde(rename = "matchedLineCount", skip_serializing_if = "Option::is_none")]
    pub total_matched_lines: Option<u32>,
    /// Count views over two or more `matchString` entries: per entry, in
    /// order, its matches (`countMatches`) or the lines holding it
    /// (`countLines`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub term_counts: Option<Vec<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pagination: Option<Paging>,
    /// The page cursor that reads this file's rows past the shown ones.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<serde_json::Value>,
    /// The file's lines, when the search counted them as it read the file
    /// ([`octocode_engine::types::SearchedSource::lines`]): where a lead's
    /// read of it ends. Never serialized.
    #[serde(skip)]
    pub(crate) lines: Option<u32>,
}

/// The response's totals. They serialize flat into the row (`matchCount`,
/// `matchedLineCount`, `filesScanned`, `binarySkipped`,
/// `fileErrors`, `partialReasons`); the fields marked `skip` are what the
/// runtime reads and the row never carries.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchStats {
    /// Occurrences; absent when the view did not count them.
    #[serde(rename = "matchCount", skip_serializing_if = "Option::is_none")]
    pub total_occurrences: Option<u32>,
    /// Matched lines; absent when the view did not count them.
    #[serde(rename = "matchedLineCount", skip_serializing_if = "Option::is_none")]
    pub matched_lines: Option<u32>,
    /// Files holding a hit over the whole scan, in the views that show hits or
    /// their counts; absent when the scan missed content (a cap, an
    /// unreadable file), since its file count is then not final.
    #[serde(rename = "matchedFiles", skip_serializing_if = "Option::is_none")]
    pub matched_files: Option<u32>,
    #[serde(skip)]
    pub files_matched: u32,
    #[serde(rename = "filesScanned")]
    pub files_searched: u32,
    #[serde(skip)]
    pub capped: Option<bool>,
    #[serde(skip)]
    pub cap_reasons: Vec<octocode_engine::types::CapReason>,
    /// The coverage gaps of the scan, one per reason: the limits that ended
    /// it early ([`cap_partial_reason`]). The row writes them through [`LocalSearchResult::to_row`].
    #[serde(skip)]
    pub partial_reasons: Vec<crate::response::paging::PartialReason>,
    #[serde(skip)]
    pub error_count: Option<u32>,
    #[serde(skip)]
    pub first_error: Option<String>,
    /// Files the search could not read, and the first of them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_errors: Option<FileErrors>,
    /// Files skipped as binary from their leading bytes, on a first page
    /// with hits (an empty page states it in `warnings`).
    #[serde(rename = "binarySkipped", skip_serializing_if = "Option::is_none")]
    pub binary_skipped: Option<u32>,
    /// What a `testCode` scope left out.
    #[serde(rename = "testCodeExcluded", skip_serializing_if = "Option::is_none")]
    pub test_code_excluded: Option<octocode_engine::types::TestCodeExcluded>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FileErrors {
    pub count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSearchResult {
    #[serde(skip)]
    pub status: SearchStatus,
    #[serde(flatten)]
    pub stats: SearchStats,
    /// `unique: "countAll"`: each matched value once over the whole scan,
    /// its occurrences and the files holding it, most frequent first.
    #[serde(rename = "uniqueValues", skip_serializing_if = "Option::is_none")]
    pub unique_values: Option<Vec<super::tally::UniqueValue>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<SearchFile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pagination: Option<Paging>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection: Option<octocode_contract_types::Selection>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
    #[serde(flatten, serialize_with = "crate::response::result::serialize_calls")]
    pub next: Option<serde_json::Value>,
    /// Some candidate content was not searched (unreadable paths, or a binary
    /// file cut at its first NUL) or is past a cursor maximum, so the returned
    /// matches are not the full set and zero matches do not prove absence.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_partial: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub terminal_limit: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped_paths: Option<octocode_contract_types::SkippedPaths>,
    #[serde(skip)]
    pub source_snapshot: Option<String>,
}

impl LocalSearchResult {
    /// The result as its row, its partial reasons written by the one
    /// partial writer ([`crate::response::paging::mark_partial`]).
    pub fn to_row(&self) -> serde_json::Result<serde_json::Value> {
        let mut row = serde_json::to_value(self)?;
        if self.status == SearchStatus::Empty
            && let Some(data) = row.as_object_mut()
        {
            // An empty row is its empty list and what the scan covered
            // (`filesScanned`, skipped and partial facts): a counter that
            // says zero again is dropped.
            for counter in ["matchCount", "matchedLineCount", "matchedFiles"] {
                if data.get(counter).and_then(serde_json::Value::as_u64) == Some(0) {
                    data.remove(counter);
                }
            }
            data.shift_insert(0, "files".to_owned(), serde_json::Value::Array(Vec::new()));
        }
        // Every matched line holds one hit when the two counts agree: the
        // line count then is the `matchCount` already stated.
        let level = |fields: &mut serde_json::Map<String, serde_json::Value>| {
            if fields.get("matchedLineCount").is_some()
                && fields.get("matchedLineCount") == fields.get("matchCount")
            {
                fields.remove("matchedLineCount");
            }
        };
        if let Some(data) = row.as_object_mut() {
            level(data);
            for file in data
                .get_mut("files")
                .and_then(serde_json::Value::as_array_mut)
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_object_mut)
            {
                level(file);
            }
        }
        for reason in &self.stats.partial_reasons {
            crate::response::paging::mark_partial(&mut row, *reason, &[]);
        }
        Ok(row)
    }
}

/// The declared partial reason of a limit that ended a text scan early.
pub(crate) fn cap_partial_reason(
    reason: octocode_engine::types::CapReason,
) -> crate::response::paging::PartialReason {
    use crate::response::paging::PartialReason;
    use octocode_engine::types::CapReason;
    match reason {
        CapReason::MaxFileSize => PartialReason::FileTooLarge,
        CapReason::Cancelled => PartialReason::Cancelled,
        CapReason::BinaryQuit => PartialReason::BinaryQuit,
    }
}
