//! Validate the requested pattern without guessing a different query.
use super::local_search_query::{self, Multiline};
use crate::response::result::ToolError;
use octocode_contract_types::{ErrorCode, LocalSearchQuery, RegexEngine};

pub(super) fn check_pattern(query: &LocalSearchQuery) -> Result<(), ToolError> {
    let mode = local_search_query::regex_mode(query);
    let pattern = local_search_query::pattern(query);
    if local_search_query::multiline(query) == Multiline::Off
        && octocode_engine::search::text_pattern::matches_line_break(
            pattern,
            mode == RegexEngine::Literal,
        )
    {
        return Err(ToolError::new(
            ErrorCode::InvalidPattern,
            "matchString matches a line break; set multiline to on to search across lines.",
        ));
    }
    if mode == RegexEngine::Literal {
        return Ok(());
    }
    let checked = octocode_engine::search::text_pattern::validate_text_pattern(
        pattern,
        false,
        mode == RegexEngine::Pcre2,
    );
    if checked.valid {
        Ok(())
    } else {
        Err(ToolError::new(
            ErrorCode::InvalidPattern,
            regex_error_message(pattern, checked.error.as_deref()),
        ))
    }
}

/// The engine compiles `matchString` inside its own wrapper group, so the raw
/// parse error echoes that internal pattern, and a PCRE2 offset counts the
/// wrapper. Report the caller's pattern and the parser's reason instead.
pub(super) fn regex_error_message(match_string: &str, raw: Option<&str>) -> String {
    let reason = raw
        .and_then(|raw| {
            raw.lines()
                .rev()
                .find_map(|line| line.trim().strip_prefix("error:"))
                .map(str::trim)
                .or_else(|| Some(raw.trim()))
        })
        .filter(|reason| !reason.is_empty())
        .unwrap_or("invalid regex pattern");
    let reason = match reason.split_once(" at offset ") {
        Some((head, tail)) => match tail.split_once(": ") {
            Some((offset, rest)) if offset.bytes().all(|b| b.is_ascii_digit()) => {
                format!("{head}: {rest}")
            }
            _ => reason.to_owned(),
        },
        None => reason.to_owned(),
    };
    format!("Invalid regex matchString `{match_string}`: {reason}")
}
