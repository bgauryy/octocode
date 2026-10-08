//! Pattern validation and runnable repairs for an invalid regex.

use super::types::*;
use crate::tools::id::ToolId;
use crate::tools::result::Continuation;
use crate::tools::result::ToolError;
use serde_json::json;

/// An invalid pattern, classified from the engine's typed validation
/// result (not from search error text) before walking the tree.
pub(super) fn check_pattern(query: &LocalSearchQuery) -> Result<(), ToolError> {
    if query.regex_mode() == LocalSearchQueryRegex::Literal {
        return Ok(());
    }
    let checked = octocode_engine::portable::validate_text_pattern(
        &query.match_string,
        false,
        query.regex_mode() == LocalSearchQueryRegex::Pcre2,
    );
    if checked.valid {
        Ok(())
    } else {
        Err(invalid_regex(
            query,
            regex_error_message(&query.match_string, checked.error.as_deref()),
        ))
    }
}

/// The engine compiles `matchString` inside its own wrapper group, so the raw
/// parse error echoes that internal pattern. Report the caller's pattern and
/// the parser's reason instead.
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
    format!("Invalid regex matchString `{match_string}`: {reason}")
}

/// `invalidPattern` with a literal-search repair continuation.
pub(super) fn invalid_regex(query: &LocalSearchQuery, message: String) -> ToolError {
    // The caller's own fields, not the runtime-normalized ones: a fresh
    // page-1 search re-derives the same view defaults (contextLines,
    // matchContentLength, pageSize), so spelling them out only adds bytes.
    let mut repaired = serde_json::to_value(query).unwrap_or_else(|_| json!({}));
    if let Some(object) = repaired.as_object_mut() {
        object.retain(|_, value| !value.is_null());
        for cursor in ["snapshot", "page", "matchPage"] {
            object.remove(cursor);
        }
    }
    let pcre2 = query.regex_mode() == LocalSearchQueryRegex::Pcre2;
    let mut hints = vec![
        "Use regex:\"literal\" for exact text, or escape metacharacters ( [ . per alternative to keep regex matching.".to_owned(),
    ];
    // `poll_(proceed|budget` means the group `poll_(proceed|budget)`: closing
    // it keeps every alternative anchored, where escaping the `(` would turn
    // `budget` into a bare, much broader alternative. Otherwise an alternation
    // stays a regex: a literal search for `a(|b(` matches nothing, while each
    // alternative escaped finds every anchor.
    let why = if let Some(text) = close_unclosed_group(&query.match_string, pcre2) {
        hints.push(format!(
            "The repair reads matchString as `{text}`; send regex:\"literal\" to match the text exactly instead."
        ));
        repaired["matchString"] = json!(text);
        "Close the unclosed group so its alternatives stay inside it."
    } else if let Some(text) = repair_alternation(&query.match_string, pcre2) {
        repaired["matchString"] = json!(text);
        "Search each alternative with its metacharacters escaped."
    } else {
        repaired["regex"] = json!("literal");
        "Search matchString as literal text."
    };
    ToolError {
        hints,
        next: Some(Box::new(json!({
            "repair": Continuation::new(ToolId::LocalSearch, repaired).why(why).build()
        }))),
        ..ToolError::new("invalidPattern", message)
    }
}

/// `x_(a|b` → `x_(a|b)`: exactly one group is left open and everything after
/// it is two or more bare word alternatives. `None` otherwise, so call syntax
/// such as `f("a"|g(` keeps the per-alternative escape.
pub(super) fn close_unclosed_group(text: &str, pcre2: bool) -> Option<String> {
    let mut open = Vec::new();
    let mut in_class = false;
    let mut chars = text.char_indices();
    while let Some((index, c)) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '[' if !in_class => in_class = true,
            ']' if in_class => in_class = false,
            '(' if !in_class => open.push(index),
            ')' if !in_class => {
                open.pop()?;
            }
            _ => {}
        }
    }
    let [start] = open.as_slice() else {
        return None;
    };
    let alternatives = text[start + 1..].split('|').collect::<Vec<_>>();
    let bare = |part: &&str| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-'))
    };
    if alternatives.len() < 2 || !alternatives.iter().all(bare) {
        return None;
    }
    let closed = format!("{text})");
    octocode_engine::portable::validate_text_pattern(&closed, false, pcre2)
        .valid
        .then_some(closed)
}

/// `a(|b|c[` → `a\(|b|c\[`: split on unescaped `|`, keep alternatives that
/// parse on their own, escape the rest. `None` for a single alternative or
/// when the joined result still fails to parse.
pub(super) fn repair_alternation(text: &str, pcre2: bool) -> Option<String> {
    let mut parts = vec![String::new()];
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let part = parts.last_mut()?;
                part.push(c);
                if let Some(next) = chars.next() {
                    part.push(next);
                }
            }
            '|' => parts.push(String::new()),
            _ => parts.last_mut()?.push(c),
        }
    }
    if parts.len() < 2 {
        return None;
    }
    let valid = |pattern: &str| {
        octocode_engine::portable::validate_text_pattern(pattern, false, pcre2).valid
    };
    let repaired = parts
        .iter()
        .map(|part| {
            if !part.is_empty() && valid(part) {
                part.clone()
            } else {
                regex::escape(part)
            }
        })
        .collect::<Vec<_>>()
        .join("|");
    valid(&repaired).then_some(repaired)
}

#[cfg(test)]
mod repair_tests {
    use super::*;

    #[test]
    fn invalid_regex_message_shows_the_callers_pattern_not_the_wrapper() {
        let message = regex_error_message(
            "(",
            Some("regex parse error:\n    (?:()\n    ^\nerror: unclosed group"),
        );
        assert_eq!(message, "Invalid regex matchString `(`: unclosed group");
        assert!(!message.contains("(?:"));
        assert_eq!(
            regex_error_message("[", None),
            "Invalid regex matchString `[`: invalid regex pattern"
        );
    }

    #[test]
    fn invalid_regex_repair_keeps_only_caller_fields() {
        let query: LocalSearchQuery = serde_json::from_value(json!({
            "path":"/tmp","matchString":"(unclosed","mainGoal": "test", "reasoning":"r","page":3,"pageSize":5
        }))
        .expect("query");
        let error = invalid_regex(&query, "unclosed group".into());
        let next = error.next.expect("repair");
        let repair = &next["repair"]["query"]["queries"][0];
        assert_eq!(repair["regex"], "literal");
        assert_eq!(repair["pageSize"], 5, "caller fields survive: {repair}");
        for derived in ["contextLines", "matchContentLength", "page", "matchPage"] {
            assert!(repair.get(derived).is_none(), "{derived} leaked: {repair}");
        }
        crate::contracts::validate_query("localSearch", repair.clone())
            .expect("repair query is contract-valid");
    }

    #[test]
    fn an_invalid_alternation_is_repaired_per_alternative_not_as_one_literal() {
        let query: LocalSearchQuery = serde_json::from_value(json!({
            "path":"/tmp","mainGoal":"g","reasoning":"r",
            "matchString":"EndProcessProperty(|SetPropertyPresence(|PropertyPresence\\.None"
        }))
        .expect("query");
        let error = invalid_regex(&query, "unclosed group".into());
        let repair = &error.next.expect("repair")["repair"]["query"]["queries"][0];
        assert_eq!(
            repair["matchString"],
            "EndProcessProperty\\(|SetPropertyPresence\\(|PropertyPresence\\.None",
            "broken alternatives are escaped, valid ones kept: {repair}"
        );
        assert!(
            repair.get("regex").is_none_or(|mode| mode == "rust"),
            "{repair}"
        );
        let text = repair["matchString"].as_str().expect("text");
        assert!(octocode_engine::portable::validate_text_pattern(text, false, false).valid);
        crate::contracts::validate_query("localSearch", repair.clone())
            .expect("repair query is contract-valid");
        // A single anchor keeps the literal repair.
        let single: LocalSearchQuery = serde_json::from_value(json!({
            "path":"/tmp","mainGoal":"g","reasoning":"r","matchString":"call("
        }))
        .expect("query");
        let error = invalid_regex(&single, "unclosed group".into());
        assert_eq!(
            error.next.expect("repair")["repair"]["query"]["queries"][0]["regex"],
            "literal"
        );
    }

    #[test]
    fn an_unclosed_group_of_bare_alternatives_is_closed_not_widened() {
        let query: LocalSearchQuery = serde_json::from_value(json!({
            "path":"/tmp","mainGoal":"g","reasoning":"r","matchString":"poll_(proceed|budget","regex":"rust"
        }))
        .expect("query");
        let error = invalid_regex(&query, "unclosed group".into());
        let repair = &error.next.expect("repair")["repair"]["query"]["queries"][0];
        assert_eq!(
            repair["matchString"], "poll_(proceed|budget)",
            "the group closes; `budget` never becomes a bare alternative: {repair}"
        );
        assert!(
            error
                .hints
                .iter()
                .any(|hint| hint.contains("regex:\"literal\"")),
            "the literal reading stays one hop away: {:?}",
            error.hints
        );
        crate::contracts::validate_query("localSearch", repair.clone())
            .expect("repair query is contract-valid");
        // Alternatives that carry call syntax keep the per-alternative escape.
        for (text, expected) in [
            (
                "capture_limited|hydrate_candidate|capture_resource(",
                "capture_limited|hydrate_candidate|capture_resource\\(",
            ),
            (
                "insert_header(\"retry-after\"|append_header(\"retry-after\"|set_body_json",
                "insert_header\\(\"retry\\-after\"|append_header\\(\"retry\\-after\"|set_body_json",
            ),
        ] {
            assert_eq!(close_unclosed_group(text, false), None, "{text}");
            assert_eq!(repair_alternation(text, false).as_deref(), Some(expected));
        }
    }

    #[test]
    fn escaped_bars_do_not_split_alternatives() {
        assert_eq!(repair_alternation("a\\|b(", false), None, "one alternative");
        assert_eq!(repair_alternation("x[|y", false).as_deref(), Some("x\\[|y"));
    }
}
