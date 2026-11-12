//! What the shared response stages ask ghGetHistoryItem about its output:
//! the shared recovery table answers for every code.
use crate::response::output::ToolOutput;

pub(crate) struct Output;

impl ToolOutput for Output {}
