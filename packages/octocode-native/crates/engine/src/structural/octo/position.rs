use serde::Deserialize;
use tree_sitter::Node;

use super::error::CompileError;
use super::matching::{CaptureEnv, ExecutionError};
use super::rule::{CompiledRule, Document, RawRule};
use crate::num::saturating_u32;
use crate::structural::language::AgLanguage;

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum Position {
    Number(u32),
    Formula(String),
}

impl Position {
    pub(super) fn compile(&self) -> Result<(i64, i64), CompileError> {
        match self {
            Self::Number(n) => Ok((0, i64::from(*n))),
            Self::Formula(text) => parse_formula(text),
        }
    }
}

pub(super) fn parse_formula(text: &str) -> Result<(i64, i64), CompileError> {
    let text: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let text = text.to_ascii_lowercase();
    let invalid =
        || CompileError::rule("invalid nthChild position: expected an integer or An+B formula");
    if text == "odd" {
        return Ok((2, 1));
    }
    if text == "even" {
        return Ok((2, 0));
    }
    let integer = |s: &str| -> Result<i64, CompileError> {
        let value = s.parse::<i32>().map_err(|_| invalid())?;
        Ok(i64::from(value))
    };
    if let Some((a, b)) = text.split_once('n') {
        let a = match a {
            "" | "+" => 1,
            "-" => -1,
            other => integer(other)?,
        };
        let b = if b.is_empty() {
            0
        } else if b.starts_with(['+', '-']) {
            integer(b)?
        } else {
            return Err(invalid());
        };
        Ok((a, b))
    } else {
        integer(&text).map(|b| (0, b))
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum RawOrdinal {
    Simple(Position),
    Detailed(OrdinalOptions),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct OrdinalOptions {
    position: Position,
    #[serde(default)]
    reverse: bool,
    of_rule: Option<Box<RawRule>>,
}

impl RawOrdinal {
    pub(super) fn filter(&self) -> Option<&RawRule> {
        match self {
            Self::Simple(_) => None,
            Self::Detailed(v) => v.of_rule.as_deref(),
        }
    }
    pub(super) fn filter_mut(&mut self) -> Option<&mut RawRule> {
        match self {
            Self::Simple(_) => None,
            Self::Detailed(v) => v.of_rule.as_deref_mut(),
        }
    }
    pub(super) fn position(&self) -> &Position {
        match self {
            Self::Simple(p) => p,
            Self::Detailed(v) => &v.position,
        }
    }
}

pub(super) struct Ordinal {
    step: i64,
    offset: i64,
    reverse: bool,
    filter: Option<Box<CompiledRule>>,
}

impl Ordinal {
    pub(super) fn compile(lang: &AgLanguage, raw: &RawOrdinal) -> Result<Self, CompileError> {
        let (step, offset) = raw.position().compile()?;
        Ok(Self {
            step,
            offset,
            reverse: matches!(raw, RawOrdinal::Detailed(v) if v.reverse),
            filter: raw
                .filter()
                .map(|rule| CompiledRule::compile_checked(lang, rule).map(Box::new))
                .transpose()?,
        })
    }

    pub(super) fn matches<'t>(
        &self,
        candidate: Node<'t>,
        ancestors: Option<&[Node<'t>]>,
        document: &Document<'_, 't>,
        captures: &mut CaptureEnv,
    ) -> Result<bool, ExecutionError> {
        let owned;
        let ancestors = match ancestors {
            Some(chain) => chain,
            None => {
                owned = document.tree.ancestors(candidate, document.deadline)?;
                &owned
            }
        };
        let Some(parent) = ancestors.last() else {
            return Ok(false);
        };
        let mut count = 0i64;
        let mut index = None;
        for child in document
            .tree
            .children(*parent)
            .into_iter()
            .filter(Node::is_named)
        {
            ExecutionError::check(document.deadline)?;
            let checkpoint = captures.checkpoint();
            let eligible = match &self.filter {
                Some(filter) => filter.matches(child, Some(ancestors), document, captures)?,
                None => true,
            };
            captures.rollback(checkpoint);
            if eligible {
                count += 1;
                if child == candidate {
                    index = Some(count);
                }
            }
        }
        let Some(index) = index else {
            return Ok(false);
        };
        let index = if self.reverse {
            count - index + 1
        } else {
            index
        };
        let delta = index - self.offset;
        let accepted = if self.step == 0 {
            delta == 0
        } else {
            delta % self.step == 0 && delta / self.step >= 0
        };
        if !accepted {
            return Ok(false);
        }
        // Only the selected sibling contributes filter captures.
        match &self.filter {
            Some(filter) => filter.matches(candidate, Some(ancestors), document, captures),
            None => Ok(true),
        }
    }
}

/// A `range` end point in the units every match row prints: a 1-based line
/// and a 1-based UTF-16 column (an end column is exclusive).
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SourcePosition {
    line: usize,
    column: Option<usize>,
}

impl SourcePosition {
    fn is_one_based(&self) -> bool {
        self.line >= 1 && self.column.is_none_or(|column| column >= 1)
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExactRange {
    start: SourcePosition,
    end: SourcePosition,
}

/// The 1-based UTF-16 column a match row prints for `point`: the same
/// conversion `line_index_support` applies to every match it emits.
fn printed_column(point: tree_sitter::Point, document: &Document<'_, '_>) -> usize {
    let column = document.lines().row_col_to_utf16_column(
        document.content,
        saturating_u32(point.row),
        saturating_u32(point.column),
    );
    usize::try_from(column).map_or(usize::MAX, |column| column.saturating_add(1))
}

impl ExactRange {
    pub(super) fn validate(&self) -> Result<(), CompileError> {
        if !(self.start.is_one_based() && self.end.is_one_based()) {
            return Err(CompileError::rule(
                "range line and column are 1-based, as match rows print them",
            ));
        }
        if self.start.line > self.end.line
            || (self.start.line == self.end.line
                && self
                    .start
                    .column
                    .zip(self.end.column)
                    .is_some_and(|(a, b)| a > b))
        {
            return Err(CompileError::rule("range start must not follow its end"));
        }
        Ok(())
    }
    pub(super) fn matches(&self, node: Node<'_>, document: &Document<'_, '_>) -> bool {
        let start = node.start_position();
        let end = node.end_position();
        if start.row.saturating_add(1) != self.start.line
            || end.row.saturating_add(1) != self.end.line
        {
            return false;
        }
        self.start
            .column
            .is_none_or(|c| printed_column(start, document) == c)
            && self
                .end
                .column
                .is_none_or(|c| printed_column(end, document) == c)
    }
}
