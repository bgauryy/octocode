//! The continuations a localFetch row leads to.
use octocode_contract_types::LocalFetchQuery;
use serde::Serialize;

/// A localFetch read a result leads to (a page, a restart, a lead); it
/// serializes through the one shared continuation builder ([`crate::response::result::Continuation`]).
#[derive(Clone, Debug, PartialEq)]
pub struct FetchCall {
    pub query: LocalFetchQuery,
    continuing: bool,
}
impl FetchCall {
    pub fn continued(query: LocalFetchQuery) -> Self {
        Self {
            query,
            continuing: true,
        }
    }
    pub fn related(query: LocalFetchQuery) -> Self {
        Self {
            query,
            continuing: false,
        }
    }
}
impl Serialize for FetchCall {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::Error;
        let row = serde_json::to_value(&self.query).map_err(S::Error::custom)?;
        if self.continuing {
            crate::response::result::Continuation::continued(crate::id::ToolId::LocalFetch, row)
        } else {
            crate::response::result::Continuation::related(crate::id::ToolId::LocalFetch, row)
        }
        .build()
        .serialize(serializer)
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct NextCalls {
    /// Recovery of a classified view without classification: the complete
    /// unfiltered view.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unfiltered: Option<FetchCall>,
    /// Exactly the unknown chunks, beside `unfiltered`, when they are
    /// narrower than the complete view.
    #[serde(
        rename = "unfilteredUnknown",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub unfiltered_unknown: Option<FetchCall>,
    /// The unknown chunks' `ranges` past what `unfilteredUnknown` reads
    /// (`unfilteredUnknown2`, `unfilteredUnknown3`, …): each a call of as
    /// many rows of `ranges` as one call takes, together reaching every
    /// unknown chunk.
    #[serde(flatten)]
    pub unfiltered_unknown_rest: serde_json::Map<String, serde_json::Value>,
    /// The same classified read judging the chunks its candidate budget
    /// left unjudged.
    #[serde(
        rename = "nextCandidatePage",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub next_candidate_page: Option<FetchCall>,
    #[serde(rename = "nextPage", skip_serializing_if = "Option::is_none")]
    pub continue_window: Option<FetchCall>,
    #[serde(rename = "readBoundedLines", skip_serializing_if = "Option::is_none")]
    pub read_bounded_lines: Option<FetchCall>,
    /// The requested `ranges` after the span a streamed multi-range page
    /// stopped inside (that span reads on through `nextPage`).
    #[serde(
        rename = "readOtherRanges",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub read_other_ranges: Option<FetchCall>,
    /// Offset-zero recovery for a page requested past the end of the view.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub restart: Option<FetchCall>,
    /// The rest of a declaration a `block` range stopped inside.
    #[serde(
        rename = "continueBlock",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub continue_block: Option<FetchCall>,
    /// A structureSearch tree of a path that is a directory, not a file.
    #[serde(rename = "viewTree", skip_serializing_if = "Option::is_none", default)]
    pub view_tree: Option<serde_json::Value>,
    /// A structureSearch listing that finds a missing file by its stem.
    #[serde(rename = "findFile", skip_serializing_if = "Option::is_none", default)]
    pub find_file: Option<serde_json::Value>,
    /// A localSearch for a missed matchString in the file's directory.
    #[serde(rename = "findText", skip_serializing_if = "Option::is_none", default)]
    pub text_search: Option<serde_json::Value>,
    /// The same read matched case-insensitively, after a case-sensitive
    /// matchString selected no line.
    #[serde(
        rename = "ignoreCase",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub ignore_case: Option<FetchCall>,
    /// The same read with one input fixed: a match read without its
    /// `ranges`, after the ranges selected no line while the file has hits
    /// outside them, or a missing file's mistyped name corrected to its one
    /// close sibling.
    #[serde(rename = "read", skip_serializing_if = "Option::is_none", default)]
    pub read: Option<FetchCall>,
    /// The same read with each regex item that does not compile repaired.
    #[serde(rename = "repair", skip_serializing_if = "Option::is_none", default)]
    pub repair: Option<FetchCall>,
}

/// Which source a continuation read reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadSource {
    /// The version this page read: a page of the same result, or the
    /// evidence a classified page withheld. Bound to it by `snapshot`.
    Page,
    /// The file as it is when the read runs: a restart, a wider context, a
    /// lead.
    Current,
}

/// The localFetch reads of [`NextCalls`]: wire name, the source it reads,
/// field. The one list of them; both accessors come from it.
macro_rules! fetch_reads {
    ($($wire:literal $source:ident $field:ident),+ $(,)?) => {
        /// Every localFetch read among the continuations, with its wire
        /// name and the source it reads.
        pub(super) fn reads(&self) -> [(&'static str, ReadSource, &Option<FetchCall>); { [$($wire),+].len() }] {
            [$(($wire, ReadSource::$source, &self.$field)),+]
        }

        /// [`Self::reads`], mutable.
        pub(super) fn reads_slots(
            &mut self,
        ) -> [(&'static str, ReadSource, &mut Option<FetchCall>); { [$($wire),+].len() }] {
            [$(($wire, ReadSource::$source, &mut self.$field)),+]
        }
    };
}

impl NextCalls {
    fetch_reads! {
        "unfiltered" Page unfiltered,
        "unfilteredUnknown" Page unfiltered_unknown,
        "nextCandidatePage" Page next_candidate_page,
        "nextPage" Page continue_window,
        "readBoundedLines" Current read_bounded_lines,
        "readOtherRanges" Page read_other_ranges,
        "restart" Current restart,
        "continueBlock" Page continue_block,
        "ignoreCase" Current ignore_case,
        "read" Current read,
        "repair" Current repair,
    }

    /// Add every continuation `other` carries; `self` keeps its own on a clash.
    pub fn absorb(&mut self, mut other: Self) {
        for ((_, _, mine), (_, _, theirs)) in
            self.reads_slots().into_iter().zip(other.reads_slots())
        {
            if mine.is_none() {
                *mine = theirs.take();
            }
        }
        for (mine, theirs) in [
            (&mut self.view_tree, other.view_tree),
            (&mut self.find_file, other.find_file),
            (&mut self.text_search, other.text_search),
        ] {
            if mine.is_none() {
                *mine = theirs;
            }
        }
        if self.unfiltered_unknown_rest.is_empty() {
            self.unfiltered_unknown_rest = other.unfiltered_unknown_rest;
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The localFetch reads among these continuations.
    pub fn reads_mut(&mut self) -> impl Iterator<Item = &mut FetchCall> {
        self.reads_slots()
            .into_iter()
            .filter_map(|(_, _, read)| read.as_mut())
    }

    /// The reads of `source`.
    pub fn reads_of_mut(&mut self, source: ReadSource) -> impl Iterator<Item = &mut FetchCall> {
        self.reads_slots()
            .into_iter()
            .filter(move |(_, reads, _)| *reads == source)
            .filter_map(|(_, _, read)| read.as_mut())
    }

    /// Whether a continuation reads more of this result (a page), as opposed
    /// to an optional lead such as `read`; only a page makes the row
    /// partial. A call of another tool is a lead, never a page.
    pub fn leaves_more(&self) -> bool {
        self.reads()
            .into_iter()
            .any(|(_, _, read)| read.as_ref().is_some_and(|read| read.continuing))
            || self
                .unfiltered_unknown_rest
                .values()
                .any(|call| call["kind"] == "continue")
    }
}
