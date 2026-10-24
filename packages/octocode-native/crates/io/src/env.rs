//! The process environment as UTF-8 text. `std::env::vars()` panics on the
//! first non-UTF-8 entry, which would let any stray variable crash the CLI;
//! the lossy form keeps every entry and replaces only the invalid bytes.

use std::collections::BTreeMap;
use std::ffi::OsString;

/// Every environment entry, invalid UTF-8 replaced rather than rejected.
pub fn vars_lossy() -> BTreeMap<String, String> {
    lossy_pairs(std::env::vars_os())
}

/// The lossy conversion over any `(name, value)` source; testable without
/// mutating the process environment.
fn lossy_pairs(pairs: impl IntoIterator<Item = (OsString, OsString)>) -> BTreeMap<String, String> {
    pairs
        .into_iter()
        .map(|(name, value)| {
            (
                name.to_string_lossy().into_owned(),
                value.to_string_lossy().into_owned(),
            )
        })
        .collect()
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;

    #[test]
    fn a_non_utf8_value_keeps_its_entry_with_replacement_bytes() {
        let pairs = [
            (OsString::from("GOOD"), OsString::from("ok")),
            (OsString::from("BAD"), OsString::from_vec(vec![0xff, b'x'])),
        ];
        let map = lossy_pairs(pairs);
        assert_eq!(map["GOOD"], "ok");
        assert_eq!(map["BAD"], "\u{fffd}x");
    }

    #[test]
    fn vars_lossy_reports_the_live_environment() {
        let live = std::env::vars_os()
            .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
            .collect::<Vec<_>>();
        let map = vars_lossy();
        let (name, value) = live.first().expect("a test process has an environment");
        assert_eq!(map.get(name), Some(value));
        assert!(
            live.iter().all(|(name, _)| map.contains_key(name)),
            "every UTF-8 entry is reported"
        );
    }

    #[test]
    fn a_non_utf8_name_keeps_its_entry() {
        let pairs = [(OsString::from_vec(vec![0xfe]), OsString::from("v"))];
        assert_eq!(lossy_pairs(pairs).len(), 1);
    }
}
