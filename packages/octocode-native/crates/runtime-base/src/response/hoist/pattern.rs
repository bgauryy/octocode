//! Row pointer patterns as the contract writes them: RFC 6901 tokens, `*`
//! for every item of an array or every member of a record (an object keyed
//! by data, such as `metavarRanges`). A pattern never reaches inside an executable
//! call (`tool` beside `input`) or a row's `next` and `related` channels, so
//! a hoist never changes a call a reader runs.
use serde_json::Value;

/// One parsed pattern: its tokens, `*` standing for any item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Pattern(Vec<String>);

/// The members no pattern enters: the calls a row offers.
pub(crate) const CALL_CHANNELS: [&str; 2] = ["next", "related"];

impl Pattern {
    /// The pattern a pointer spells (`""` is the root itself).
    pub(crate) fn parse(pointer: &str) -> Self {
        Self(
            pointer
                .split('/')
                .skip(1)
                .map(crate::json_pointer::unescape)
                .collect(),
        )
    }

    pub(crate) fn tokens(&self) -> &[String] {
        &self.0
    }

    /// This pattern below `prefix`, or `None` when it does not start with
    /// `prefix`'s tokens (a `*` matches only a `*`).
    pub(crate) fn below(&self, prefix: &Self) -> Option<Self> {
        self.0
            .strip_prefix(prefix.0.as_slice())
            .map(|rest| Self(rest.to_vec()))
    }

    /// The pattern of the object holding this pattern's member; the root has
    /// none.
    pub(crate) fn parent(&self) -> Option<Self> {
        let (_, parent) = self.0.split_last()?;
        Some(Self(parent.to_vec()))
    }

    /// Every concrete path the pattern reaches in `value`, in document
    /// order, with the token (an item index or a member key) each `*` bound.
    pub(crate) fn matches(&self, value: &Value) -> Vec<Match> {
        let mut found = Vec::new();
        walk(value, &self.0, &mut Vec::new(), &mut Vec::new(), &mut found);
        found
    }

    /// This pattern with its leading `*`s bound to `bound` (as a [`Match`]
    /// of another pattern bound them): the one path it names there.
    pub(crate) fn bind(&self, bound: &[String]) -> Option<Vec<String>> {
        let mut stars = bound.iter();
        self.0
            .iter()
            .map(|token| {
                if token == "*" {
                    stars.next().cloned()
                } else {
                    Some(token.clone())
                }
            })
            .collect()
    }
}

/// One place a pattern reaches: its path and the token each `*` bound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Match {
    pub(crate) path: Vec<String>,
    pub(crate) bound: Vec<String>,
}

impl Match {
    pub(crate) fn pointer(&self) -> String {
        crate::json_pointer::of(&self.path)
    }
}

/// Whether `object` is a call a reader runs whole.
pub(crate) fn is_call(object: &serde_json::Map<String, Value>) -> bool {
    crate::contracts::output_fields::executable_call(object)
}

fn walk(
    value: &Value,
    tokens: &[String],
    path: &mut Vec<String>,
    bound: &mut Vec<String>,
    found: &mut Vec<Match>,
) {
    let Some((token, rest)) = tokens.split_first() else {
        found.push(Match {
            path: path.clone(),
            bound: bound.clone(),
        });
        return;
    };
    let mut enter = |key: String, child: &Value, star: bool| {
        path.push(key.clone());
        if star {
            bound.push(key);
        }
        walk(child, rest, path, bound, found);
        if star {
            bound.pop();
        }
        path.pop();
    };
    match value {
        Value::Object(object) if !is_call(object) => {
            if token == "*" {
                for (key, child) in object {
                    if !CALL_CHANNELS.contains(&key.as_str()) {
                        enter(key.clone(), child, true);
                    }
                }
            } else if !CALL_CHANNELS.contains(&token.as_str())
                && let Some(child) = object.get(token.as_str())
            {
                enter(token.clone(), child, false);
            }
        }
        Value::Array(items) if token == "*" => {
            for (index, item) in items.iter().enumerate() {
                enter(index.to_string(), item, true);
            }
        }
        Value::Array(items) => {
            if let Some(item) = token.parse::<usize>().ok().and_then(|at| items.get(at)) {
                enter(token.clone(), item, false);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
