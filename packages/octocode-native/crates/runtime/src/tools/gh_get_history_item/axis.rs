//! The pagination axes of an item read: each `contentPagination` key, the
//! `next.*` page that continues it, the cursor it carries, the sections a
//! page continuation reads, and how its leftover is worded. One table, so
//! promotion, notes and shaping never spell an axis apart.
use super::Section;

/// One `contentPagination` axis of a pull request or issue page.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum Axis {
    Body,
    ReviewBody,
    Patches,
    CommentBody,
    Files,
    Comments,
    ReviewComments,
    Reviews,
    Commits,
}

/// What an axis's page continues from: a char offset into a text window,
/// or a page number of a collection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Cursor {
    Offset,
    Page,
}

impl Cursor {
    /// The page field holding the cursor.
    pub(super) fn field(self) -> &'static str {
        match self {
            Self::Offset => "nextOffset",
            Self::Page => "nextPage",
        }
    }

    /// The query key the cursor continues.
    pub(super) fn query_key(self) -> &'static str {
        match self {
            Self::Offset => "offset",
            Self::Page => "page",
        }
    }
}

impl Axis {
    const ALL: [Self; 9] = [
        Self::Body,
        Self::ReviewBody,
        Self::Patches,
        Self::CommentBody,
        Self::Files,
        Self::Comments,
        Self::ReviewComments,
        Self::Reviews,
        Self::Commits,
    ];

    /// The `contentPagination` key.
    pub(super) fn key(self) -> &'static str {
        match self {
            Self::Body => "body",
            Self::ReviewBody => "reviewBody",
            Self::Patches => "patches",
            Self::CommentBody => "commentBody",
            Self::Files => "files",
            Self::Comments => "comments",
            Self::ReviewComments => "reviewComments",
            Self::Reviews => "reviews",
            Self::Commits => "commits",
        }
    }

    /// The axis a `contentPagination` key names.
    pub(super) fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|axis| axis.key() == key)
    }

    /// The `next.*` page that continues the axis.
    pub(super) fn next_name(self) -> &'static str {
        match self {
            Self::Body => "continueBody",
            Self::ReviewBody => "continueReviewBody",
            Self::Patches => "continuePatch",
            Self::CommentBody => "continueCommentBody",
            Self::Files => "nextFilePage",
            Self::Comments => "nextCommentPage",
            Self::ReviewComments => "nextReviewCommentPage",
            Self::Reviews => "nextReviewPage",
            Self::Commits => "nextCommitPage",
        }
    }

    pub(super) fn cursor(self) -> Cursor {
        match self {
            Self::Body | Self::ReviewBody | Self::Patches | Self::CommentBody => Cursor::Offset,
            Self::Files | Self::Comments | Self::ReviewComments | Self::Reviews | Self::Commits => {
                Cursor::Page
            }
        }
    }

    /// The sections a page of a collection axis continues (of those the
    /// read named); text axes narrow by their own rules.
    pub(super) fn own_sections(self) -> &'static [Section] {
        match self {
            Self::Files => &[Section::Files, Section::Patches],
            Self::Comments => &[Section::Comments],
            Self::ReviewComments => &[Section::ReviewComments],
            Self::Reviews => &[Section::Reviews],
            Self::Commits => &[Section::Commits, Section::CommitFiles],
            Self::Body | Self::ReviewBody | Self::Patches | Self::CommentBody => &[],
        }
    }

    /// What a char of a text axis is a char of.
    pub(super) fn text_label(self) -> Option<&'static str> {
        Some(match self {
            Self::Body => "body",
            Self::ReviewBody => "review body",
            Self::CommentBody => "comment body",
            Self::Patches => "patch",
            _ => return None,
        })
    }

    /// What one item of a collection axis is.
    pub(super) fn noun(self) -> Option<(&'static str, &'static str)> {
        Some(match self {
            Self::Files => ("changed file", "changed files"),
            Self::Comments => ("comment", "comments"),
            Self::ReviewComments => ("review comment", "review comments"),
            Self::Reviews => ("review", "reviews"),
            Self::Commits => ("commit", "commits"),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Axis, Cursor};

    /// Every axis parses back from its key; text axes continue an offset,
    /// collections a page, and only collections count items.
    #[test]
    fn every_axis_round_trips_and_names_its_cursor() {
        for axis in Axis::ALL {
            assert_eq!(Axis::parse(axis.key()), Some(axis));
            let text = axis.cursor() == Cursor::Offset;
            assert_eq!(text, axis.text_label().is_some(), "{axis:?}");
            assert_eq!(text, axis.noun().is_none(), "{axis:?}");
            assert_eq!(text, axis.own_sections().is_empty(), "{axis:?}");
        }
        assert_eq!(Axis::parse("commitFiles"), None);
    }
}
