//! The wire shape shared by opaque-handle issuance, parsing and page sizing.
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

const PREFIX: &str = "cursor2.";
pub(crate) const ID_BYTES: usize = 16;
const ID_CHARS: usize = (ID_BYTES * 8).div_ceil(6);

pub(crate) fn encode(bytes: &[u8; ID_BYTES]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub(crate) fn format(set: &str, member: &str) -> String {
    format!("{PREFIX}{set}{member}")
}

pub(crate) fn parts(token: &str) -> Option<(&str, &str)> {
    let encoded = token.strip_prefix(PREFIX)?;
    if encoded.len() != ID_CHARS * 2 || !encoded.is_ascii() {
        return None;
    }
    let (set, member) = encoded.split_at(ID_CHARS);
    for id in [set, member] {
        let mut bytes = [0; ID_BYTES];
        if URL_SAFE_NO_PAD.decode_slice(id, &mut bytes).ok()? != ID_BYTES {
            return None;
        }
    }
    Some((set, member))
}

/// A sizing stand-in; publication replaces it with a retained handle.
pub(crate) fn placeholder() -> String {
    let id = encode(&[0; ID_BYTES]);
    format(&id, &id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_format_keeps_exact_ids_and_rejects_malformed_handles() {
        let set = URL_SAFE_NO_PAD.encode([0xab; ID_BYTES]);
        let member = URL_SAFE_NO_PAD.encode([0xcd; ID_BYTES]);
        let token = format(&set, &member);
        assert_eq!(token.len(), 52);
        assert!(token.starts_with("cursor2."));
        assert_eq!(parts(&token), Some((set.as_str(), member.as_str())));
        assert_eq!(placeholder().len(), token.len());
        for malformed in [
            format!("{token}A"),
            token[1..].to_owned(),
            format!("cursor1.{}", "0".repeat(64)),
        ] {
            assert!(parts(&malformed).is_none());
        }
    }

    #[test]
    fn each_component_requires_canonical_unpadded_url_safe_encoding() {
        let zero = "A".repeat(22);
        let valid = format(&zero, &zero);
        assert!(parts(&valid).is_some());
        for malformed in [
            format(&format!("{}B", "A".repeat(21)), &zero),
            format(&zero, &format!("{}B", "A".repeat(21))),
            format(&format!("{}=", "A".repeat(21)), &zero),
            format(&format!("{}+", "A".repeat(21)), &zero),
            format(&format!("{}/", "A".repeat(21)), &zero),
            format!("cursor2.{}AA", "界".repeat(14)),
            format!("{valid}=="),
        ] {
            assert!(parts(&malformed).is_none(), "accepted {malformed}");
        }
        let high = "_____________________w";
        assert_eq!(parts(&format(high, &zero)), Some((high, zero.as_str())));
    }
}
