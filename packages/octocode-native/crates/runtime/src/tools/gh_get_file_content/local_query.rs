//! The localFetch extraction query a GitHub file query maps onto one to one.
use super::GhGetFileContentQuery;
use crate::tools::source_view::LocalFetchQuery;
use octocode_github::ProviderError;

/// The GitHub file query is the localFetch extraction query plus repository
/// coordinates (`owner`, `repo`, `ref`, `forceRefresh`): the extraction
/// fields map one to one onto the localFetch wire type, so both tools share
/// one extraction request. Paged reads get the default page size.
pub(super) fn local_query(query: &GhGetFileContentQuery) -> Result<LocalFetchQuery, ProviderError> {
    let GhGetFileContentQuery {
        block,
        case_mode,
        context_lines,
        extras,
        force_refresh: _,
        full_content,
        length,
        main_goal,
        match_string,
        minify,
        offset,
        owner: _,
        path,
        ranges,
        reasoning,
        clasify_context,
        selection,
        ref_: _,
        regex,
        repo: _,
        unit,
        whole_word,
    } = query;
    Ok(LocalFetchQuery {
        block: *block,
        case_mode: *case_mode,
        whole_word: *whole_word,
        context_lines: *context_lines,
        extras: extras.clone(),
        full_content: *full_content,
        length: *length,
        main_goal: main_goal
            .as_ref()
            .map(|goal| goal.as_str().parse())
            .transpose()
            .map_err(decode_display)?,
        match_string: match_string.clone(),
        minify: *minify,
        offset: *offset,
        path: path.as_str().parse().map_err(decode_display)?,
        ranges: ranges
            .iter()
            .map(|range| range.as_str().parse())
            .collect::<Result<Vec<_>, _>>()
            .map_err(decode_display)?,
        reasoning: reasoning
            .as_ref()
            .map(|reasoning| reasoning.as_str().parse())
            .transpose()
            .map_err(decode_display)?,
        regex: *regex,
        snapshot: None,
        clasify_context: clasify_context
            .as_ref()
            .map(|context| serde_json::to_value(context).and_then(serde_json::from_value))
            .transpose()
            .map_err(decode_display)?,
        selection: selection
            .as_ref()
            .map(|plan| plan.as_str().parse())
            .transpose()
            .map_err(decode_display)?,
        unit: *unit,
    })
}

pub(super) fn decode_display(error: impl std::fmt::Display) -> ProviderError {
    ProviderError::new(
        octocode_github::ProviderErrorKind::Decode,
        error.to_string(),
    )
}
