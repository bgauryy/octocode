//! The wire shape shared by opaque-handle issuance, parsing and page sizing.
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

const PREFIX: &str = "cursor2.";
pub const ID_BYTES: usize = 16;
const ID_CHARS: usize = (ID_BYTES * 8).div_ceil(6);

pub fn encode(bytes: &[u8; ID_BYTES]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn format(set: &str, member: &str) -> String {
    format!("{PREFIX}{set}{member}")
}

pub fn parts(token: &str) -> Option<(&str, &str)> {
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
pub fn placeholder() -> String {
    let id = encode(&[0; ID_BYTES]);
    format(&id, &id)
}

#[cfg(test)]
mod tests;
