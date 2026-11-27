//! The one compact JSON projection sent by CLI `--json` and MCP text.

use crate::security::Sanitized;

/// Serialize the sealed response envelope without another wire copy.
#[must_use]
pub fn wire_json(content: &Sanitized) -> String {
    content.to_string()
}
