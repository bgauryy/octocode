use crate::security::content::key_blocks::MAX_KEY_BLOCK_RANGES;
use crate::security::{key_formats, marker_lines};

use super::*;
mod block_edges;
mod key_blocks;
mod key_format_blocks;
mod literal_markers;
mod marker_precheck;
mod query_terms;
mod sanitization;
mod span_property;
mod structured;
mod symbol_leads;
