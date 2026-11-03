//! Typed ghGetFileContent reads one tool offers into GitHub file content:
//! the file, and an optional match, line window or plain start. A value the
//! contract rejects (an empty path, a zero line) yields no read.
use octocode_contract_types::{
    CaseMode, GhGetFileContentQuery, GhGetFileContentQueryContent0,
    GhGetFileContentQueryContent0View, GhGetFileContentQueryContentMatchString,
    GhGetFileContentQueryContentMatchStringContextLines,
    GhGetFileContentQueryContentMatchStringView, GhGetFileContentQueryContentRanges,
    GhGetFileContentQueryContentRangesView, RegexEngine, SourceLineRange,
};
use std::num::NonZeroU64;

/// The file a read names: `ref` unset reads the default branch.
pub(crate) struct Target<'a> {
    pub(crate) owner: &'a str,
    pub(crate) repo: &'a str,
    pub(crate) path: &'a str,
    pub(crate) reference: Option<&'a str>,
}

/// How a read matches text: the terms, and how they are read.
pub(crate) struct Terms {
    pub(crate) terms: Vec<String>,
    pub(crate) case_mode: CaseMode,
    pub(crate) regex_engine: RegexEngine,
}

impl Terms {
    /// `terms` as plain, smart-case literals.
    pub(crate) fn literal(terms: Vec<String>) -> Self {
        Self {
            terms,
            case_mode: CaseMode::Smart,
            regex_engine: RegexEngine::Literal,
        }
    }
}

pub(crate) fn line_range(start: u32, end: u32) -> Option<SourceLineRange> {
    Some(SourceLineRange {
        start_line: NonZeroU64::new(u64::from(start))?,
        end_line: NonZeroU64::new(u64::from(end))?,
    })
}

/// A ghGetFileContent read of `target` for `terms`, `context` lines around
/// each match (unset: the contract's), whole declarations with `block`.
pub(crate) fn match_read(
    target: &Target<'_>,
    terms: Terms,
    context: Option<(i64, i64)>,
    block: bool,
) -> Option<GhGetFileContentQuery> {
    let terms_list = terms
        .terms
        .iter()
        .map(|term| term.as_str().try_into().ok())
        .collect::<Option<Vec<_>>>()?;
    Some(
        GhGetFileContentQueryContentMatchString {
            block: block.then_some(true),
            case_mode: terms.case_mode,
            classification: None,
            context_lines: context.map(|(before, after)| {
                GhGetFileContentQueryContentMatchStringContextLines { after, before }
            }),
            main_goal: None,
            match_string: terms_list.into(),
            owner: target.owner.try_into().ok()?,
            path: target.path.try_into().ok()?,
            ranges: Vec::new(),
            read_progress: None,
            reasoning: None,
            ref_: target.reference.map(TryInto::try_into).transpose().ok()?,
            regex_engine: terms.regex_engine,
            repo: target.repo.try_into().ok()?,
            selection: None,
            view: GhGetFileContentQueryContentMatchStringView::Content,
            whole_word: false,
        }
        .into(),
    )
}

/// A ghGetFileContent read of `target`'s start.
pub(crate) fn plain_read(target: &Target<'_>) -> Option<GhGetFileContentQuery> {
    Some(
        GhGetFileContentQueryContent0 {
            classification: None,
            main_goal: None,
            owner: target.owner.try_into().ok()?,
            path: target.path.try_into().ok()?,
            read_progress: None,
            reasoning: None,
            ref_: target.reference.map(TryInto::try_into).transpose().ok()?,
            repo: target.repo.try_into().ok()?,
            selection: None,
            view: GhGetFileContentQueryContent0View::Content,
        }
        .into(),
    )
}

/// A ghGetFileContent read of `target`'s inclusive line `start..=end`.
pub(crate) fn range_read(
    target: &Target<'_>,
    start: u32,
    end: u32,
) -> Option<GhGetFileContentQuery> {
    Some(
        GhGetFileContentQueryContentRanges {
            block: None,
            classification: None,
            main_goal: None,
            owner: target.owner.try_into().ok()?,
            path: target.path.try_into().ok()?,
            ranges: vec![line_range(start, end)?],
            read_progress: None,
            reasoning: None,
            ref_: target.reference.map(TryInto::try_into).transpose().ok()?,
            repo: target.repo.try_into().ok()?,
            selection: None,
            view: GhGetFileContentQueryContentRangesView::Content,
        }
        .into(),
    )
}
