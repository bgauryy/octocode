//! The continuations a localFetch row leads to.
use octocode_contract_types::LocalFetchQuery;
use serde::Serialize;

/// A localFetch read a result leads to (a page, a restart, a lead); it
/// serializes through the one shared continuation builder ([`octocode_runtime_base::response::result::Continuation`]).
#[derive(Clone, Debug, PartialEq)]
pub struct FetchCall {
    pub query: LocalFetchQuery,
    continuing: bool,
    /// The row's own page walk (`next.nextPage`): built and checked by
    /// [`octocode_runtime_base::response::collection::walk_query`].
    walk: bool,
    /// A related read the row needs ([`Continuation::required`]).
    ///
    /// [`Continuation::required`]: octocode_runtime_base::response::result::Continuation::required
    required: bool,
}
impl FetchCall {
    /// The call that reads the next page of the result (`next.nextPage`).
    pub fn page(query: LocalFetchQuery) -> Self {
        Self {
            query,
            continuing: true,
            walk: true,
            required: false,
        }
    }
    pub fn continued(query: LocalFetchQuery) -> Self {
        Self {
            query,
            continuing: true,
            walk: false,
            required: false,
        }
    }
    pub fn related(query: LocalFetchQuery) -> Self {
        Self {
            query,
            continuing: false,
            walk: false,
            required: false,
        }
    }
    /// A related read of text the row was asked for and does not state:
    /// it stays on an evidence row.
    pub fn needed(query: LocalFetchQuery) -> Self {
        Self {
            required: true,
            ..Self::related(query)
        }
    }
}
impl Serialize for FetchCall {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.walk {
            octocode_runtime_base::response::collection::walk_query(self.query.clone())
        } else if self.continuing {
            octocode_runtime_base::response::result::Continuation::continued_query(
                self.query.clone(),
            )
        } else {
            let call = octocode_runtime_base::response::result::Continuation::related_query(
                self.query.clone(),
            );
            if self.required { call.required() } else { call }
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
    /// The same read replaying the selection as exactly the units it left
    /// undecided, beside `unfiltered`, unbilled.
    #[serde(
        rename = "unfilteredUnknown",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub unfiltered_unknown: Option<FetchCall>,
    /// The same classified read judging the chunks its candidate budget
    /// left unjudged.
    #[serde(
        rename = "nextCandidatePage",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub next_candidate_page: Option<FetchCall>,
    /// The same classified read with `classification.maxRequests` raised to
    /// the ceiling of the plan that exceeded it (nothing was sent); billed.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub confirm: Option<FetchCall>,
    #[serde(rename = "nextPage", skip_serializing_if = "Option::is_none")]
    pub continue_window: Option<FetchCall>,
    #[serde(rename = "readBoundedLines", skip_serializing_if = "Option::is_none")]
    pub read_bounded_lines: Option<FetchCall>,
    /// Offset-zero recovery for a page requested past the end of the view.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub restart: Option<FetchCall>,
    /// A structureSearch tree of a path that is a directory, not a file.
    #[serde(rename = "viewTree", skip_serializing_if = "Option::is_none", default)]
    pub view_tree: Option<serde_json::Value>,
    /// The same read matched case-insensitively, after a case-sensitive
    /// matchString selected no line.
    #[serde(
        rename = "ignoreCase",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub ignore_case: Option<FetchCall>,
    /// The same file read with one input changed: a match read without its
    /// `ranges`, after the ranges selected no line while the file has hits
    /// outside them; after a miss, the names a declaration head declares
    /// alone or the file's `symbols` outline; or a missing file's mistyped
    /// name corrected to its one close sibling.
    #[serde(rename = "read", skip_serializing_if = "Option::is_none", default)]
    pub read: Option<FetchCall>,
    /// A localSearch for a missed matchString in the file's directory: after
    /// every read of the file itself, which a miss tries first.
    #[serde(rename = "findText", skip_serializing_if = "Option::is_none", default)]
    pub text_search: Option<serde_json::Value>,
    /// The same read with each regex item that does not compile repaired.
    #[serde(rename = "repair", skip_serializing_if = "Option::is_none", default)]
    pub repair: Option<FetchCall>,
    /// The ghCloneRepo call that checks a missing path out beside the sparse
    /// checkout that leaves it out (a related call: it rides `related[]`).
    #[serde(rename = "reclone", skip_serializing_if = "Option::is_none", default)]
    pub reclone: Option<serde_json::Value>,
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
        "confirm" Page confirm,
        "nextPage" Page continue_window,
        "readBoundedLines" Current read_bounded_lines,
        "restart" Current restart,
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
            (&mut self.text_search, other.text_search),
            (&mut self.reclone, other.reclone),
        ] {
            if mine.is_none() {
                *mine = theirs;
            }
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

    /// Whether a continuation reads evidence this result left unread, as
    /// opposed to paging on (`nextPage`: pagination, not partial
    /// coverage), an optional lead such as `read`, or `unfiltered`
    /// (what a finished judgment chose to leave out, which is the request, not a
    /// gap). Other continuing reads (unjudged candidates, other ranges) are
    /// unread evidence. A call of another tool is a lead, never a page.
    pub fn leaves_unread(&self) -> bool {
        self.reads().into_iter().any(|(name, _, read)| {
            !matches!(name, "nextPage" | "unfiltered")
                && read.as_ref().is_some_and(|read| read.continuing)
        })
    }
}
