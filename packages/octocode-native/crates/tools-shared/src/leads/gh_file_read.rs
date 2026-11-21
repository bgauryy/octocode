//! Typed ghGetFileContent reads one tool offers into GitHub file content:
//! the file, and an optional match, line window or plain start. A value the
//! contract rejects (an empty path, a zero line) yields no read.
use octocode_contract_types::{
    CaseMode, ContentView, ContextLines, GhGetFileContentQuery, GhGetFileContentQueryContent0,
    GhGetFileContentQueryContentMatchString, RegexEngine,
};

/// The file a read names: `ref` unset reads the default branch.
pub struct Target<'a> {
    pub owner: &'a str,
    pub repo: &'a str,
    pub path: &'a str,
    pub reference: Option<&'a str>,
}

/// How a read matches text: the terms, and how they are read.
pub struct Terms {
    pub terms: Vec<String>,
    pub case_mode: CaseMode,
    pub regex_engine: RegexEngine,
}

impl Terms {
    /// `terms` as plain, smart-case literals.
    pub fn literal(terms: Vec<String>) -> Self {
        Self {
            terms,
            case_mode: CaseMode::Smart,
            regex_engine: RegexEngine::Literal,
        }
    }
}

/// A ghGetFileContent read of `target` for `terms`, `context` lines around
/// each match (unset: the contract's), whole declarations with `block`.
pub fn match_read(
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
            context_lines: context.map(|(before, after)| ContextLines { after, before }),
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
            view: ContentView::Content,
            whole_word: false,
        }
        .into(),
    )
}

/// A ghGetFileContent read of `target`'s start.
pub fn plain_read(target: &Target<'_>) -> Option<GhGetFileContentQuery> {
    Some(
        GhGetFileContentQueryContent0 {
            main_goal: None,
            owner: target.owner.try_into().ok()?,
            path: target.path.try_into().ok()?,
            read_progress: None,
            reasoning: None,
            ref_: target.reference.map(TryInto::try_into).transpose().ok()?,
            repo: target.repo.try_into().ok()?,
            view: ContentView::Content,
        }
        .into(),
    )
}
