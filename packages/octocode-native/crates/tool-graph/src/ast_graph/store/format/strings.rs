//! The string dictionary of graph tables. Built tables keep the builder's
//! strings as they are; decoded tables hold every string end to end in one
//! buffer, so a decode costs one growing allocation, not one per string.

/// Strings by id.
#[derive(Clone, Debug)]
pub(crate) struct Strings(Store);

#[derive(Clone, Debug)]
enum Store {
    /// The builder's interned strings, moved in: a build holds each
    /// string's bytes once.
    Built(Vec<String>),
    /// A decoded dictionary. `ends[id]`: the byte after string `id`.
    Packed { text: String, ends: Vec<usize> },
}

impl Default for Strings {
    fn default() -> Self {
        Self(Store::Packed {
            text: String::new(),
            ends: Vec::new(),
        })
    }
}

impl PartialEq for Strings {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}

impl Eq for Strings {}

impl Strings {
    pub(crate) fn len(&self) -> usize {
        match &self.0 {
            Store::Built(strings) => strings.len(),
            Store::Packed { ends, .. } => ends.len(),
        }
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The bytes of every string together.
    pub(crate) fn text_len(&self) -> usize {
        match &self.0 {
            Store::Built(strings) => strings.iter().map(String::len).sum(),
            Store::Packed { text, .. } => text.len(),
        }
    }

    pub(crate) fn get(&self, id: usize) -> Option<&str> {
        match &self.0 {
            Store::Built(strings) => strings.get(id).map(String::as_str),
            Store::Packed { text, ends } => {
                let end = *ends.get(id)?;
                let start = id.checked_sub(1).map_or(0, |previous| ends[previous]);
                text.get(start..end)
            }
        }
    }

    pub(crate) fn push(&mut self, value: impl AsRef<str>) {
        match &mut self.0 {
            Store::Built(strings) => strings.push(value.as_ref().to_owned()),
            Store::Packed { text, ends } => {
                text.push_str(value.as_ref());
                ends.push(text.len());
            }
        }
    }

    pub(crate) fn reserve(&mut self, count: usize) {
        match &mut self.0 {
            Store::Built(strings) => strings.reserve(count),
            Store::Packed { ends, .. } => ends.reserve(count),
        }
    }

    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = &str> + DoubleEndedIterator {
        (0..self.len()).map(|id| &self[id])
    }

    /// The packed buffer, packing built strings first: decoding appends
    /// only to a packed dictionary.
    fn packed(&mut self) -> (&mut String, &mut Vec<usize>) {
        if let Store::Built(strings) = &mut self.0 {
            let mut text = String::with_capacity(strings.iter().map(String::len).sum());
            let mut ends = Vec::with_capacity(strings.len());
            for value in std::mem::take(strings) {
                text.push_str(&value);
                ends.push(text.len());
            }
            self.0 = Store::Packed { text, ends };
        }
        match &mut self.0 {
            Store::Packed { text, ends } => (text, ends),
            Store::Built(_) => unreachable!("built strings were packed above"),
        }
    }

    /// Appends one front-coded entry: the first `shared` bytes of string
    /// `previous` (`None`: the empty string), then `suffix`. Only the bytes
    /// from the last character boundary of that prefix on are checked as
    /// UTF-8; the prefix before it is a checked string's.
    pub(crate) fn push_front_coded(
        &mut self,
        previous: Option<usize>,
        shared: usize,
        suffix: &[u8],
    ) -> Result<(), String> {
        let (text, ends) = self.packed();
        let prefix = match previous {
            Some(id) => {
                let end = *ends.get(id).ok_or("graph string table is corrupt")?;
                id.checked_sub(1).map_or(0, |previous| ends[previous])..end
            }
            None => 0..0,
        };
        if shared > prefix.len() {
            return Err("graph string table is corrupt".into());
        }
        let at = prefix.start + shared;
        // Strictly after `previous`: past the shared bytes, the suffix
        // orders after the rest of `previous` (an empty first string is the
        // only one with nothing before it).
        if previous.is_some() && suffix <= &text.as_bytes()[at..prefix.end] {
            return Err("graph string table is not sorted".into());
        }
        // Two strings may share only the leading bytes of a character: the
        // prefix then ends inside it and the suffix holds the rest.
        let boundary = text.floor_char_boundary(at);
        if boundary == at {
            let suffix =
                std::str::from_utf8(suffix).map_err(|_| "graph string table is not UTF-8")?;
            text.extend_from_within(prefix.start..at);
            text.push_str(suffix);
        } else {
            let mut tail = text.as_bytes()[boundary..at].to_vec();
            tail.extend_from_slice(suffix);
            let tail = String::from_utf8(tail).map_err(|_| "graph string table is not UTF-8")?;
            text.extend_from_within(prefix.start..boundary);
            text.push_str(&tail);
        }
        ends.push(text.len());
        Ok(())
    }

    /// Replaces each `(id, string)` pair's string; for an id given more than
    /// once, the last pair wins. Built strings are replaced in place; a
    /// packed buffer is rebuilt once.
    pub(crate) fn replace(&mut self, replacements: impl IntoIterator<Item = (usize, String)>) {
        match &mut self.0 {
            Store::Built(strings) => {
                for (id, value) in replacements {
                    if let Some(slot) = strings.get_mut(id) {
                        *slot = value;
                    }
                }
            }
            Store::Packed { .. } => {
                let mut replacements = replacements.into_iter().collect::<Vec<_>>();
                if replacements.is_empty() {
                    return;
                }
                // Stable: pairs of one id keep their order, so the last wins.
                replacements.sort_by_key(|(id, _)| *id);
                let mut next = replacements.into_iter().peekable();
                let mut rebuilt = Self::default();
                let (text, ends) = rebuilt.packed();
                text.reserve(self.text_len());
                ends.reserve(self.len());
                for id in 0..self.len() {
                    let mut value = None;
                    while let Some((_, replacement)) = next.next_if(|(at, _)| *at == id) {
                        value = Some(replacement);
                    }
                    text.push_str(value.as_deref().unwrap_or(&self[id]));
                    ends.push(text.len());
                }
                *self = rebuilt;
            }
        }
    }
}

/// The builder's interned strings, moved in without a copy.
impl From<Vec<String>> for Strings {
    fn from(values: Vec<String>) -> Self {
        Self(Store::Built(values))
    }
}

impl std::ops::Index<usize> for Strings {
    type Output = str;

    /// Panics for an id past the dictionary, as a `Vec` index does.
    fn index(&self, id: usize) -> &str {
        match &self.0 {
            Store::Built(strings) => &strings[id],
            Store::Packed { text, ends } => {
                let start = id.checked_sub(1).map_or(0, |previous| ends[previous]);
                // Every end is a character boundary: each entry was checked
                // as UTF-8 when it was appended, so `get` always finds it.
                text.get(start..ends[id]).unwrap_or_default()
            }
        }
    }
}

impl<S: AsRef<str>> FromIterator<S> for Strings {
    fn from_iter<I: IntoIterator<Item = S>>(values: I) -> Self {
        let mut strings = Self::default();
        strings.extend(values);
        strings
    }
}

impl<S: AsRef<str>> Extend<S> for Strings {
    fn extend<I: IntoIterator<Item = S>>(&mut self, values: I) {
        for value in values {
            self.push(value.as_ref());
        }
    }
}

#[cfg(test)]
mod tests;
