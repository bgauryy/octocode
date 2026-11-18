//! The one meaning of a `matchString` list in every text tool: a line that
//! holds any of its entries. localFetch reads it in place and localSearch
//! runs it as the alternation built here.

/// The regex alternation of `entries`, each `(text, case_sensitive)` keeping
/// its own case rule as an inline group (`(?i:text)`). Literal entries are
/// escaped first; `is_regex` entries are patterns of their own and run as
/// written.
pub fn alternation<'a>(
    entries: impl IntoIterator<Item = (&'a str, bool)>,
    is_regex: bool,
) -> String {
    entries
        .into_iter()
        .map(|(text, case_sensitive)| {
            let flag = if case_sensitive { "" } else { "i" };
            if is_regex {
                format!("(?{flag}:{text})")
            } else {
                format!("(?{flag}:{})", regex::escape(text))
            }
        })
        .collect::<Vec<_>>()
        .join("|")
}

#[cfg(test)]
mod tests;
