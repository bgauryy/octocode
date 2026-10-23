//! The runnable repair of a read whose regex `matchString` does not compile.

use super::types::{FetchCall, LocalFetchQuery, MatchString, ReadRegex};
use crate::tools::source_view::types::local_fetch_query;

/// Whether `pattern` compiles under the read's own regex engine.
fn compiles(pattern: &str, pcre2: bool) -> bool {
    octocode_engine::search::text_pattern::validate_text_pattern(pattern, false, pcre2).valid
}

/// One invalid item, repaired: its one unclosed group of bare alternatives
/// closed, else its alternatives escaped one by one, else escaped whole.
fn repair_item(item: &str, pcre2: bool) -> String {
    crate::regex_repair::close_unclosed_group(item, pcre2)
        .or_else(|| crate::regex_repair::repair_alternation(item, pcre2))
        .filter(|repaired| compiles(repaired, pcre2))
        .unwrap_or_else(|| regex::escape(item))
}

/// The same read, repaired; `None` when the read is not a regex or every item
/// compiles. A Rust-regex read whose items all compile as PCRE2 (backreferences,
/// lookaround) is the same read as `regex:"pcre2"`, the engine that owns them.
/// Otherwise every item that does not compile is repaired (the items that
/// compile stay as written).
pub(super) fn repair(q: &LocalFetchQuery) -> Option<FetchCall> {
    if !local_fetch_query::is_regex(q) {
        return None;
    }
    let pcre2 = q.regex == Some(ReadRegex::Pcre2);
    let items = local_fetch_query::match_strings(q);
    if items.iter().all(|item| compiles(item, pcre2)) {
        return None;
    }
    if crate::regex_repair::only_pcre2_compiles(items.iter().copied(), pcre2) {
        let mut query = q.clone();
        query.regex = Some(ReadRegex::Pcre2);
        return Some(FetchCall { query });
    }
    let repaired = items
        .iter()
        .map(|item| {
            if compiles(item, pcre2) {
                (*item).to_owned()
            } else {
                repair_item(item, pcre2)
            }
        })
        .collect::<Vec<_>>();
    let mut query = q.clone();
    query.match_string = Some(match repaired.as_slice() {
        [one] => MatchString::String(one.parse().ok()?),
        _ => MatchString::Array(
            repaired
                .iter()
                .map(|item| item.parse().ok())
                .collect::<Option<Vec<_>>>()?,
        ),
    });
    Some(FetchCall { query })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::source_view::types::test_local_fetch_query;

    fn read(regex: ReadRegex, items: serde_json::Value) -> LocalFetchQuery {
        LocalFetchQuery {
            path: "a.py".parse().expect("path"),
            match_string: Some(serde_json::from_value(items).expect("matchString")),
            regex: Some(regex),
            ..test_local_fetch_query()
        }
    }

    #[test]
    fn an_unclosed_call_is_escaped_and_valid_items_stay() {
        let q = read(
            ReadRegex::Rust,
            serde_json::json!(["^class ", "    def route("]),
        );
        let fixed = repair(&q).expect("repair");
        assert_eq!(
            local_fetch_query::match_strings(&fixed.query),
            vec!["^class ", "    def route\\("]
        );
        assert_eq!(fixed.query.regex, Some(ReadRegex::Rust));
    }

    #[test]
    fn an_unclosed_group_of_bare_alternatives_is_closed() {
        let q = read(ReadRegex::Rust, serde_json::json!("poll_(proceed|budget"));
        let fixed = repair(&q).expect("repair");
        assert_eq!(
            local_fetch_query::match_strings(&fixed.query),
            vec!["poll_(proceed|budget)"]
        );
    }

    #[test]
    fn what_only_pcre2_compiles_is_the_same_read_as_pcre2() {
        let q = read(ReadRegex::Rust, serde_json::json!(["^fn ", r"(\w)\1"]));
        let fixed = repair(&q).expect("repair");
        assert_eq!(fixed.query.regex, Some(ReadRegex::Pcre2));
        assert_eq!(
            local_fetch_query::match_strings(&fixed.query),
            vec!["^fn ", r"(\w)\1"]
        );
        // A PCRE2 read that fails is not retried as itself.
        let q = read(ReadRegex::Pcre2, serde_json::json!("def route("));
        assert_eq!(
            repair(&q).expect("repair").query.regex,
            Some(ReadRegex::Pcre2)
        );
    }

    #[test]
    fn valid_or_literal_reads_need_no_repair() {
        assert!(repair(&read(ReadRegex::Rust, serde_json::json!("^fn "))).is_none());
        assert!(repair(&read(ReadRegex::Literal, serde_json::json!("route("))).is_none());
    }
}
