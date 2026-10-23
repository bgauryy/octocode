//! A literal `matchString` that reads as a regex alternation, and the
//! regex leads of a literal search.

use super::super::local_search_query;
use super::*;

impl Found<'_> {
    /// The regex reading of a literal first page whose `matchString` reads
    /// as an alternation ([`reads_as_alternation`]) and compiles: its engine
    /// and the warning, when one bounded count of it over the query's own
    /// walk finds more matched lines than the literal search.
    pub(super) fn alternation_reading(
        &self,
        search_options: &TextSearchOptions,
        cancel: &impl CancellationCheck,
    ) -> Option<(&'static str, String)> {
        let query = self.query;
        let literal = self.scanned.matched_lines?;
        if self.empty
            || !self.walk_complete
            || !self.first_page()
            || query.regex.is_some()
            || query.invert_match == Some(true)
            || !reads_as_alternation(local_search_query::pattern(query))
        {
            return None;
        }
        let engine = regex_engine(local_search_query::pattern(query))?;
        let mut options = search_options.probe(SearchOutput::CountLines);
        options.syntax = if engine == "pcre2" {
            PatternSyntax::Pcre2
        } else {
            PatternSyntax::Regex
        };
        if octocode_engine::search::text_pattern::matches_line_break(
            local_search_query::pattern(query),
            false,
        ) {
            options.multiline = Some(true);
        }
        let (found, cut) = probe_search(
            options,
            self.paths,
            query.default_excludes.defaults(),
            cancel,
        )?;
        let lines = found.stats.matched_lines.filter(|lines| *lines > literal)?;
        let floor = if cut { "at least " } else { "" };
        Some((
            engine,
            format!(
                "matchString ran as literal text ({}); with regex:\"{engine}\" it matches {floor}{lines}.",
                crate::response::pages::counted(
                    u64::from(literal),
                    "matched line",
                    "matched lines"
                )
            ),
        ))
    }
}

/// Whether `text` read as a regex is an alternation: a `|` outside escapes
/// and character classes splits it into alternatives none of which is
/// empty.
pub(super) fn reads_as_alternation(text: &str) -> bool {
    // Chars in each alternative so far.
    let mut alternatives = vec![0usize];
    let mut chars = text.chars();
    let mut class = false;
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '[' => class = true,
            ']' => class = false,
            '|' if !class => {
                alternatives.push(0);
                continue;
            }
            _ => {}
        }
        if let Some(last) = alternatives.last_mut() {
            *last += 1;
        }
    }
    alternatives.len() > 1 && alternatives.iter().all(|chars| *chars > 0)
}

/// The engine `text` compiles in as a regex: `rust`, else `pcre2`.
pub(super) fn regex_engine(text: &str) -> Option<&'static str> {
    use octocode_engine::search::text_pattern::validate_text_pattern;
    if validate_text_pattern(text, false, false).valid {
        Some("rust")
    } else {
        validate_text_pattern(text, false, true)
            .valid
            .then_some("pcre2")
    }
}

/// The same search with `matchString` read as a regex under `engine`, and
/// across lines when the regex matches a line break, so the lead runs.
pub(super) fn regex_lead(query: &LocalSearchQuery, engine: &str) -> Value {
    restart_lead(query, |fields| {
        fields.insert("regex".into(), json!(engine));
        if query.multiline == LocalSearchQueryMultiline::Off
            && octocode_engine::search::text_pattern::matches_line_break(
                local_search_query::pattern(query),
                false,
            )
        {
            fields.insert("multiline".into(), json!("on"));
        }
    })
}
