use serde_json::Value;

/// One `name` value; all-lowercase text compares without case.
pub(super) struct NameEntry {
    pub(super) text: String,
    pub(super) fold: bool,
    /// The entry equals some declaration, so it matches only equal names.
    pub(super) exact: bool,
}

impl NameEntry {
    pub(super) fn new(text: String) -> Self {
        let fold = !text.chars().any(char::is_uppercase);
        Self {
            text,
            fold,
            exact: false,
        }
    }
    pub(super) fn equals(&self, name: &str) -> bool {
        if self.fold {
            name.to_lowercase() == self.text
        } else {
            name == self.text
        }
    }
    /// Whether `name` only contains this exact entry: a declaration the
    /// exact match left out.
    fn hides(&self, name: &str) -> bool {
        self.exact
            && !self.equals(name)
            && if self.fold {
                name.to_lowercase().contains(&self.text)
            } else {
                name.contains(&self.text)
            }
    }
    fn matches(&self, name: &str) -> bool {
        if self.exact {
            self.equals(name)
        } else if self.fold {
            name.to_lowercase().contains(&self.text)
        } else {
            name.contains(&self.text)
        }
    }
}

pub(super) struct NameFilter {
    pub(super) entries: Vec<NameEntry>,
}

impl NameFilter {
    /// An entry that equals a declaration name keeps only equal names.
    pub(super) fn settle<'a>(&mut self, names: impl Iterator<Item = &'a str> + Clone) {
        for entry in &mut self.entries {
            entry.exact = names.clone().any(|name| entry.equals(name));
        }
    }
    fn matches(&self, name: &str) -> bool {
        self.entries.iter().any(|entry| entry.matches(name))
    }
    fn equals(&self, name: &str) -> bool {
        self.entries.iter().any(|entry| entry.equals(name))
    }
    pub(super) fn hides(&self, name: &str) -> bool {
        self.entries.iter().any(|entry| entry.hides(name))
    }
    /// Whether `source` spells some entry's text: a declaration whose name
    /// matches an entry spells it in the source.
    pub(super) fn spelled_in(&self, source: &str) -> bool {
        let mut folded = None;
        self.entries.iter().any(|entry| {
            if entry.fold {
                folded
                    .get_or_insert_with(|| source.to_lowercase())
                    .contains(&entry.text)
            } else {
                source.contains(&entry.text)
            }
        })
    }
}

/// A row's names a `symbolName` entry can select: its name, and a trait
/// impl's type name.
pub(super) fn row_names(row: &Value) -> impl Iterator<Item = &str> {
    [row["name"].as_str(), row["typeName"].as_str()]
        .into_iter()
        .flatten()
}

/// Whether some entry equals the row's name (or trait-impl type name).
pub(super) fn names_equal(filter: &NameFilter, row: &Value) -> bool {
    row_names(row).any(|name| filter.equals(name))
}

/// A trait impl is shown as `Trait for Type`; its type name (`typeName`)
/// still selects it, as it selects the type's inherent impl.
pub(super) fn names_match(filter: &NameFilter, row: &Value) -> bool {
    row_names(row).any(|name| filter.matches(name))
}
