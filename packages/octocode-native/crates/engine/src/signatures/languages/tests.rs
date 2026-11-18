use super::*;
use std::collections::HashSet;
use tree_sitter::Parser;

/// The one grammar `selector` names by name, language id or extension;
/// `None` when it names none, or several.
fn select_one(selector: &str) -> Option<&'static LanguageEntry> {
    let selector = selector.trim();
    let mut named = LANGUAGE_TABLE.iter().filter(|entry| names(entry, selector));
    let entry = named.next()?;
    named.next().is_none().then_some(entry)
}

mod admission;
mod outlines;
mod parsing;
mod registry;
