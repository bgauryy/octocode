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
mod tests {
    use super::Strings;

    #[test]
    fn front_coded_entries_rebuild_each_string() {
        let mut strings = Strings::default();
        strings.push_front_coded(None, 0, b"alpha").unwrap();
        strings.push_front_coded(Some(0), 3, b"ine").unwrap();
        // "è" (c3 a8) then "é" (c3 a9): only the leading byte is shared.
        strings
            .push_front_coded(Some(1), 0, "è".as_bytes())
            .unwrap();
        strings.push_front_coded(Some(2), 1, &[0xa9]).unwrap();
        assert_eq!(
            strings.iter().collect::<Vec<_>>(),
            ["alpha", "alpine", "è", "é"]
        );
        assert!(strings.push_front_coded(Some(3), 3, b"x").is_err());
        assert!(strings.push_front_coded(Some(3), 1, &[0xff]).is_err());
        assert!(strings.push_front_coded(Some(9), 0, b"x").is_err());
        assert_eq!(strings.len(), 4);
    }

    /// A crafted entry that keeps only the leading byte of a multi-byte
    /// character and appends a byte no character continues with is refused,
    /// never a panic; the same holds for a whole section read.
    #[test]
    fn an_entry_splitting_a_character_is_refused() {
        let mut strings = Strings::default();
        strings.push_front_coded(None, 0, "é".as_bytes()).unwrap();
        assert_eq!(
            strings.push_front_coded(Some(0), 1, &[0xff]),
            Err("graph string table is not UTF-8".into())
        );
        assert_eq!(strings.iter().collect::<Vec<_>>(), ["é"]);
        // "é", then one shared byte and 0xff: the same entry in a section.
        let section = [2, 0, 2, 0xc3, 0xa9, 1, 1, 0xff];
        let mut t = super::super::GraphTables::default();
        assert_eq!(
            super::super::read_strings(
                &mut super::super::Reader::new(&section, "STRS"),
                &mut t,
                &super::super::LIMITS
            ),
            Err("graph string table is not UTF-8".into())
        );
    }

    /// A section is strictly sorted: string ids order its strings.
    #[test]
    fn an_entry_not_after_its_predecessor_is_refused() {
        let mut strings = Strings::default();
        strings.push_front_coded(None, 0, b"").unwrap();
        strings.push_front_coded(Some(0), 0, b"beta").unwrap();
        for (shared, suffix) in [(0, &b"alpha"[..]), (4, b""), (1, b"a"), (2, b"t")] {
            assert_eq!(
                strings.push_front_coded(Some(1), shared, suffix),
                Err("graph string table is not sorted".into()),
                "{shared} {suffix:?}"
            );
        }
        strings.push_front_coded(Some(1), 4, b"x").unwrap();
        assert_eq!(strings.iter().collect::<Vec<_>>(), ["", "beta", "betax"]);
    }

    /// A build holds each string's bytes once: the builder's strings are
    /// moved in, and a replacement edits only its own string.
    #[test]
    fn built_strings_are_moved_in_and_replaced_in_place() {
        let values = vec!["alpha".to_owned(), "beta".to_owned(), "gamma".to_owned()];
        let pointers = values
            .iter()
            .map(|value| value.as_ptr())
            .collect::<Vec<_>>();
        let mut strings = Strings::from(values);
        assert_eq!(
            strings.iter().map(str::as_ptr).collect::<Vec<_>>(),
            pointers
        );
        strings.replace([(1, "b".to_owned())]);
        assert_eq!(strings[0].as_ptr(), pointers[0]);
        assert_eq!(strings[2].as_ptr(), pointers[2]);
        assert_eq!(strings.iter().collect::<Vec<_>>(), ["alpha", "b", "gamma"]);
    }

    /// Several replacements of one id: the last one given wins, in both
    /// representations.
    #[test]
    fn the_last_replacement_of_an_id_wins() {
        let built = Strings::from(vec!["a".to_owned(), "b".to_owned()]);
        let decoded = ["a", "b"].into_iter().collect::<Strings>();
        for mut strings in [built, decoded] {
            strings.replace((0..64).map(|round| (0, format!("x{round}"))));
            assert_eq!(strings.iter().collect::<Vec<_>>(), ["x63", "b"]);
        }
    }

    #[test]
    fn replacements_keep_every_other_string() {
        let mut strings = ["a", "bb", "c"].into_iter().collect::<Strings>();
        strings.replace([(2, "z".to_owned()), (0, "yy".to_owned())]);
        assert_eq!(strings.iter().collect::<Vec<_>>(), ["yy", "bb", "z"]);
        assert_eq!(strings, ["yy", "bb", "z"].into_iter().collect::<Strings>());
    }
}
