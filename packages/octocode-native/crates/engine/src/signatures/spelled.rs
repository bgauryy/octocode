//! Where a word is spelled in a text: which syntax nodes can hold it, so a walk
//! looking for that word need not enter the rest.

/// Every place `word` starts in a text, overlapping ones included.
pub(crate) struct Spelled {
    starts: Vec<usize>,
    len: usize,
}

impl Spelled {
    pub(crate) fn of(content: &str, word: &str) -> Self {
        let mut starts = Vec::new();
        if let Some(first) = word.chars().next() {
            let mut from = 0;
            while let Some(at) = content[from..].find(word) {
                starts.push(from + at);
                from += at + first.len_utf8();
            }
        }
        Self {
            starts,
            len: word.len(),
        }
    }

    /// Whether bytes `start..end` of the text hold the word whole. Every span
    /// holds an empty word.
    pub(crate) fn within(&self, start: usize, end: usize) -> bool {
        if self.len == 0 {
            return true;
        }
        let next = self.starts.partition_point(|at| *at < start);
        self.starts.get(next).is_some_and(|at| at + self.len <= end)
    }
}

#[cfg(test)]
mod tests;
