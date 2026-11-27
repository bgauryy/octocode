//! Validate the requested pattern without guessing a different query.
use super::local_search_query::{self, Multiline};
use octocode_contract_types::{ErrorCode, LocalSearchQuery, RegexEngine};
use octocode_runtime_base::response::result::ToolError;

pub(super) fn check_pattern(query: &LocalSearchQuery) -> Result<(), ToolError> {
    check_expression(query, local_search_query::pattern(query))
}

/// Validate one expression under the query's regex and multiline rules,
/// without copying the query or changing its pattern list.
pub(super) fn check_expression(query: &LocalSearchQuery, pattern: &str) -> Result<(), ToolError> {
    let mode = local_search_query::regex_mode(query);
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
        return Ok(());
    }
    let mut error = ToolError::new(
        ErrorCode::InvalidPattern,
        regex_error_message(pattern, checked.error.as_deref()),
    );
    // The fix of this cause, which only this check knows: a pattern the
    // other regex engine compiles needs that engine, not another spelling.
    let needs_pcre2 = mode == RegexEngine::Rust
        && octocode_engine::search::text_pattern::validate_text_pattern(pattern, false, true).valid;
    error.hints.push(
        if needs_pcre2 {
            NEEDS_PCRE2_HINT
        } else {
            MALFORMED_REGEX_HINT
        }
        .to_owned(),
    );
    Err(error)
}

/// The pattern is valid for PCRE2 only (a look-around, a backreference).
pub(super) const NEEDS_PCRE2_HINT: &str = "The pattern uses a feature only the PCRE2 engine has (the error names it): set regexEngine to pcre2 and keep matchString unchanged.";

/// The pattern compiles in neither regex engine.
pub(super) const MALFORMED_REGEX_HINT: &str = "Correct the regex named in the error. If the intended match is literal text, set regexEngine to literal and keep matchString unchanged.";

/// `fileType` names a type of ripgrep's default table, as the walk selects
/// it. A name that is none is refused with the types whose files it names
/// (`rs` names files of type `rust`), or the field that selects by name.
pub(super) fn check_file_type(query: &LocalSearchQuery) -> Result<(), ToolError> {
    let Some(name) = local_search_query::file_type(query).filter(|name| !name.is_empty()) else {
        return Ok(());
    };
    let Err(reason) = octocode_tools_shared::leads::unparsed::select_file_type(name) else {
        return Ok(());
    };
    let mut error = ToolError::new(ErrorCode::InvalidInput, reason);
    let types = octocode_tools_shared::leads::unparsed::file_types_of(&format!("x.{name}"));
    error.hints.push(if types.is_empty() {
        format!(
            "fileType takes a ripgrep type name, and none holds files named *.{name}; select them by name with include: [\"*.{name}\"]."
        )
    } else {
        let quoted = types
            .iter()
            .map(|name| format!("\"{name}\""))
            .collect::<Vec<_>>();
        format!(
            "fileType takes a ripgrep type name, not an extension: files named *.{name} are of type {}.",
            quoted.join(" or ")
        )
    });
    Err(error)
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
