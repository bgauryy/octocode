//! The engine's tree-sitter parse: one worker-local parser, bounded by its
//! caller's deadline or stop signal and by the linear work bound.

mod parse;

pub(crate) use parse::{
    ParseFailure, Work, measure, parse_before, parse_ranges_before, parse_ranges_ending,
    parse_until_stopped, parse_with_deadline, whole_file_deadline,
};
