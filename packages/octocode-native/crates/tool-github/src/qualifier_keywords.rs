//! Keywords spelled as GitHub search qualifiers (`language:rust`,
//! `stars:>1000`, `author:ada`). A keyword matches text, so such a keyword
//! finds nothing a field would: the empty answer names the field that
//! carries the qualifier.

/// A search qualifier and how the tool's own fields spell it.
pub(crate) struct Qualifier {
    /// The qualifier names GitHub's search syntax knows it by (compared
    /// without case).
    pub(crate) names: &'static [&'static str],
    /// The instruction for the qualifier's value: the field to set.
    pub(crate) field: fn(&str) -> String,
}

/// `set `{field}` to `{value}``: the instruction for a field that takes the
/// qualifier's value as written.
pub(crate) fn set_to(field: &str, value: &str) -> String {
    format!("set `{field}` to `{value}`")
}

/// A [`Qualifier`] whose field takes the value as written
///
macro_rules! qualifier {
    ([$($name:literal),+] => range $field:literal) => {
        $crate::qualifier_keywords::Qualifier {
            names: &[$($name),+],
            field: |value| $crate::qualifier_keywords::set_range($field, value),
        }
    };
    ([$($name:literal),+] => $field:literal ($shape:literal)) => {
        $crate::qualifier_keywords::Qualifier {
            names: &[$($name),+],
            field: |value| {
                format!(
                    "set `{}` ({}; the qualifier's value is `{value}`)",
                    $field, $shape
                )
            },
        }
    };
    ([$($name:literal),+] => $field:literal) => {
        $crate::qualifier_keywords::Qualifier {
            names: &[$($name),+],
            field: |value| $crate::qualifier_keywords::set_to($field, value),
        }
    };
}
pub(crate) use qualifier;

/// A hint per keyword spelled as one of `qualifiers` (a name, a colon, a
/// value) that names the field carrying it.
pub(crate) fn hints(keywords: &[&str], qualifiers: &[Qualifier]) -> Vec<String> {
    keywords
        .iter()
        .filter_map(|keyword| {
            let (key, value) = keyword.trim().split_once(':')?;
            let value = value.trim();
            if value.is_empty() {
                return None;
            }
            let qualifier = qualifiers
                .iter()
                .find(|qualifier| qualifier.names.iter().any(|name| name.eq_ignore_ascii_case(key)))?;
            Some(format!(
                "Keyword `{keyword}` matches text, not the {key} qualifier: {} and drop it from keywords.",
                (qualifier.field)(value)
            ))
        })
        .collect()
}

/// A numeric qualifier value (`>1000`, `>=10`, `<5`, `<=5`, `10..50`, `7`) as
/// the `{min, max}` object a range field takes. `None` for a value that is
/// not a count or range.
pub(crate) fn count_range(value: &str) -> Option<String> {
    let number = |text: &str| text.trim().parse::<u64>().ok();
    let (min, max) = if let Some(rest) = value.strip_prefix(">=") {
        (Some(number(rest)?), None)
    } else if let Some(rest) = value.strip_prefix('>') {
        (Some(number(rest)?.checked_add(1)?), None)
    } else if let Some(rest) = value.strip_prefix("<=") {
        (None, Some(number(rest)?))
    } else if let Some(rest) = value.strip_prefix('<') {
        (None, Some(number(rest)?.checked_sub(1)?))
    } else if let Some((low, high)) = value.split_once("..") {
        let bound = |text: &str| match text.trim() {
            "" | "*" => Some(None),
            digits => number(digits).map(Some),
        };
        (bound(low)?, bound(high)?)
    } else {
        let exact = number(value)?;
        (Some(exact), Some(exact))
    };
    let mut parts = vec![];
    if let Some(min) = min {
        parts.push(format!("\"min\":{min}"));
    }
    if let Some(max) = max {
        parts.push(format!("\"max\":{max}"));
    }
    (!parts.is_empty()).then(|| format!("{{{}}}", parts.join(",")))
}

/// The instruction for a count-range field: the object computed from the
/// qualifier's value, else the `{min, max}` shape.
pub(crate) fn set_range(field: &str, value: &str) -> String {
    match count_range(value) {
        Some(range) => format!("set `{field}` to `{range}`"),
        None => format!("set `{field}` to a `{{min, max}}` range of integers"),
    }
}

#[cfg(test)]
mod tests;
