//! JSON pointer (RFC 6901) reference tokens: the one escape and unescape of
//! `~` and `/` every response walk uses.

/// `key` as one reference token: `~` as `~0`, `/` as `~1`.
pub(crate) fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// The key one reference token spells: `~1` as `/`, then `~0` as `~`.
pub(crate) fn unescape(token: &str) -> String {
    token.replace("~1", "/").replace("~0", "~")
}

/// The pointer to `path`, one token per key.
pub(crate) fn of(path: &[String]) -> String {
    path.iter().map(|key| format!("/{}", escape(key))).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_key_round_trips_through_its_token() {
        for key in ["a", "a/b", "~", "~1", "/~0/", ""] {
            assert_eq!(super::unescape(&super::escape(key)), key);
        }
        assert_eq!(super::escape("~/"), "~0~1");
        assert_eq!(super::of(&["a/b".to_owned(), "~".to_owned()]), "/a~1b/~0");
    }
}
