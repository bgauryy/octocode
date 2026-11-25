//! The byte cap on clean clone checkouts, read from the `storage.cloneCacheLimit` setting.

/// The cap when the setting is invalid: the setting's own default.
pub const DEFAULT_CLONE_CAP_BYTES: u64 = 5 << 30;

/// `text` as a byte count: a whole number with an optional binary unit
/// (`B`, `K`/`KB`/`KiB`, `M`…, `G`…, `T`…, any case). `None` for anything else,
/// including a count that overflows.
#[must_use]
pub fn parse_byte_size(text: &str) -> Option<u64> {
    let text = text.trim();
    let digits = text
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(text.len());
    let (number, unit) = text.split_at(digits);
    let shift = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 0,
        "k" | "kb" | "kib" => 10,
        "m" | "mb" | "mib" => 20,
        "g" | "gb" | "gib" => 30,
        "t" | "tb" | "tib" => 40,
        _ => return None,
    };
    number.parse::<u64>().ok()?.checked_mul(1 << shift)
}

/// The cap `setting` states (0: no cap). An invalid value is raised as a
/// notice and the default cap applies.
#[must_use]
pub fn clone_cap_from_setting(setting: &str) -> u64 {
    parse_byte_size(setting).unwrap_or_else(|| {
        octocode_io::notice::raise(
            "cloneCacheLimit",
            format!(
                "storage.cloneCacheLimit '{setting}' is not a byte count such as 5GiB or 0; the default cap applies."
            ),
        );
        DEFAULT_CLONE_CAP_BYTES
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_scale_by_powers_of_1024() {
        assert_eq!(parse_byte_size("0"), Some(0));
        assert_eq!(parse_byte_size("7"), Some(7));
        assert_eq!(parse_byte_size(" 3 b "), Some(3));
        assert_eq!(parse_byte_size("2KiB"), Some(2 << 10));
        assert_eq!(parse_byte_size("512mib"), Some(512 << 20));
        assert_eq!(parse_byte_size("5GiB"), Some(DEFAULT_CLONE_CAP_BYTES));
        assert_eq!(parse_byte_size("5G"), parse_byte_size("5gb"));
        assert_eq!(parse_byte_size("1TB"), Some(1 << 40));
    }

    #[test]
    fn invalid_sizes_are_refused() {
        for bad in [
            "",
            "GiB",
            "-1",
            "1.5GiB",
            "5 parsecs",
            "99999999999999999999",
        ] {
            assert_eq!(parse_byte_size(bad), None, "{bad}");
        }
        assert_eq!(parse_byte_size("18446744073709551615TiB"), None);
    }
}
