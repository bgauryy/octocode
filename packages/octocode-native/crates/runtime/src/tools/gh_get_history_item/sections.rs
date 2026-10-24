//! The sections one history-item read returns.
use super::{GhGetHistoryItemQuery, gh_get_history_item_query};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Section {
    Body,
    Files,
    Patches,
    Comments,
    ReviewComments,
    Reviews,
    Commits,
    CommitFiles,
}
impl Section {
    fn name(self) -> &'static str {
        match self {
            Self::Body => "body",
            Self::Files => "files",
            Self::Patches => "patches",
            Self::Comments => "comments",
            Self::ReviewComments => "reviewComments",
            Self::Reviews => "reviews",
            Self::Commits => "commits",
            Self::CommitFiles => "commitFiles",
        }
    }
}
impl std::fmt::Display for Section {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}
use serde_json::Value;

/// The sections one read returns: the generated `sections`, with the one
/// default resolved (a `matchString` with no section named reads the
/// patches it searches). Continuations set it back on the wire in the
/// published order ([`Sections::wire`]).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Sections(u8);

impl Sections {
    /// Every section, in the published order.
    const ORDER: [Section; 8] = [
        Section::Body,
        Section::Files,
        Section::Patches,
        Section::Comments,
        Section::ReviewComments,
        Section::Reviews,
        Section::Commits,
        Section::CommitFiles,
    ];

    fn bit(section: Section) -> u8 {
        let index = Self::ORDER
            .iter()
            .position(|known| *known == section)
            .unwrap_or_default();
        1 << index
    }
    /// The sections named.
    pub(crate) fn of(sections: impl IntoIterator<Item = Section>) -> Self {
        sections.into_iter().fold(Self::default(), Self::with)
    }
    /// The sections `query` reads.
    pub(super) fn resolve(query: &GhGetHistoryItemQuery) -> Self {
        let names = gh_get_history_item_query::section_names(query);
        let named = Self::of(
            Self::ORDER
                .into_iter()
                .filter(|s| names.iter().any(|name| name == s.name())),
        );
        if named.is_empty() && gh_get_history_item_query::match_strings(query).is_some() {
            Self::of([Section::Patches])
        } else {
            named
        }
    }
    pub(crate) fn has(self, section: Section) -> bool {
        self.0 & Self::bit(section) != 0
    }
    pub(crate) fn with(self, section: Section) -> Self {
        Self(self.0 | Self::bit(section))
    }
    pub(crate) fn is_empty(self) -> bool {
        self.0 == 0
    }
    /// The read lists changed files: as an inventory or with patches.
    pub(crate) fn lists_files(self) -> bool {
        self.has(Section::Files) || self.has(Section::Patches)
    }
    /// The read lists commits (with or without each commit's paths).
    pub(crate) fn lists_commits(self) -> bool {
        self.has(Section::Commits) || self.has(Section::CommitFiles)
    }
    /// The published `sections` value, in published order; `None` when
    /// empty (the summary read).
    pub(crate) fn wire(self) -> Option<Value> {
        let names = Self::ORDER
            .into_iter()
            .filter(|section| self.has(*section))
            .map(|section| Value::from(section.to_string()))
            .collect::<Vec<_>>();
        (!names.is_empty()).then_some(Value::Array(names))
    }
    /// Set these sections on a working continuation query (or clear them
    /// for the summary read).
    pub(crate) fn set_on(self, query: &mut Value) {
        if let Some(fields) = query.as_object_mut() {
            match self.wire() {
                Some(sections) => fields.insert("sections".into(), sections),
                None => fields.remove("sections"),
            };
        }
    }
}
