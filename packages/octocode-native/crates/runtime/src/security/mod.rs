mod content;
pub mod scan;
mod walk;

pub use content::{ContentSecurity, ValidationResult};
pub(crate) use content::{
    key_fragment_placeholder, match_window_intersects_key_block, private_key_block_line_ranges,
    redact_private_key_blocks, snippet_may_hold_key_material,
};
pub use octocode_engine::security::types::SanitizationResult;
pub use walk::sanitize_json;
