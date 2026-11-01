//! Outline shaping: import runs, multi-line signatures, annotation runs,
//! closing rows and docstrings collapse to what an outline lists.

/// Top-level import-like statements across the registry's grammars.
fn is_import_kind(kind: &str) -> bool {
    matches!(
        kind,
        "import_statement"
            | "import_from_statement"
            | "future_import_statement"
            | "import_declaration"
            | "import_header"
            | "use_declaration"
            | "extern_crate_declaration"
            | "preproc_include"
            | "using_directive"
            | "import"
    )
}

/// 0-based row spans of consecutive top-level imports; comments between
/// imports do not end a run.
pub(super) fn import_runs(root: tree_sitter::Node<'_>) -> Vec<(usize, usize)> {
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut open = false;
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        let kind = child.kind();
        if is_import_kind(kind) {
            let (start, end) = (child.start_position().row, child.end_position().row);
            match runs.last_mut() {
                Some(run) if open => run.1 = end,
                _ => runs.push((start, end)),
            }
            open = true;
        } else if !crate::signatures::node_kinds::is_comment_kind(kind) {
            open = false;
        }
    }
    runs
}

/// Replace each import run holding more than one kept line with one line:
/// the run's first word and its 1-based line range.
pub(super) fn collapse_import_runs(
    kept: Vec<(usize, String)>,
    runs: &[(usize, usize)],
) -> Vec<(usize, String)> {
    let mut out = Vec::with_capacity(kept.len());
    let mut index = 0;
    while index < kept.len() {
        let line = kept[index].0;
        let Some(&(start, end)) = runs
            .iter()
            .find(|(start, end)| (start + 1..=end + 1).contains(&line))
        else {
            out.push(kept[index].clone());
            index += 1;
            continue;
        };
        let run_end = kept[index..]
            .iter()
            .take_while(|(line, _)| *line <= end + 1)
            .count();
        let members = &kept[index..index + run_end];
        let count = members
            .iter()
            .filter(|(_, text)| !text.trim().is_empty())
            .count();
        if count <= 1 {
            out.extend(members.iter().cloned());
        } else {
            let first_word = members
                .iter()
                .find_map(|(_, text)| text.split_whitespace().next())
                .unwrap_or("import");
            out.push((
                line,
                format!("{first_word} … (lines {}-{})", start + 1, end + 1),
            ));
        }
        index += run_end;
    }
    out
}

/// Marks a kept outline line whose continuation was cut.
pub(super) const ELIDED: &str = " …";

/// Drop the rows of a multi-line parameter list after its first row, up to
/// the row that opens the body, and mark the first row. `head_end` is the
/// last row of the signature.
pub(super) fn collapse_signature(
    function: tree_sitter::Node<'_>,
    head_end: usize,
    keep: &mut [bool],
    elided: &mut [bool],
) {
    let Some(parameters) = function.child_by_field_name("parameters") else {
        return;
    };
    let first = parameters.start_position().row;
    let last = head_end.min(keep.len().saturating_sub(1));
    if first < last {
        keep[first + 1..=last].fill(false);
        elided[first] = true;
    }
}

/// Shrink each run of two or more rows that hold only annotations (one
/// multi-row attribute, or several stacked on one item) to its first row,
/// marked [`ELIDED`]: an outline lists the item, and the item's line range
/// reads the annotations in full.
pub(super) fn collapse_annotation_runs(
    root: tree_sitter::Node<'_>,
    lines: &[&str],
    keep: &mut [bool],
    elided: &mut [bool],
) {
    let mut annotation_rows = vec![false; lines.len()];
    let mut cursor = root.walk();
    'walk: loop {
        let node = cursor.node();
        let mut descend = true;
        if crate::signatures::node_kinds::is_annotation_kind(node.kind()) {
            descend = false;
            let (start, end) = (node.start_position(), node.end_position());
            let alone = lines.get(start.row).is_some_and(|row| {
                row.get(..start.column)
                    .is_some_and(|before| before.trim().is_empty())
            }) && lines.get(end.row).is_some_and(|row| {
                row.get(end.column..)
                    .is_some_and(|after| after.trim().is_empty())
            });
            if alone {
                let last = end.row.min(lines.len().saturating_sub(1));
                annotation_rows[start.row..=last].fill(true);
            }
        }
        if descend && cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                break 'walk;
            }
        }
    }
    let mut row = 0;
    while row < lines.len() {
        if !(keep[row] && annotation_rows[row]) {
            row += 1;
            continue;
        }
        let run = (row..lines.len())
            .take_while(|at| keep[*at] && annotation_rows[*at])
            .count();
        if run > 1 {
            keep[row + 1..row + run].fill(false);
            elided[row] = true;
        }
        row += run;
    }
}

/// Drop rows that hold only closing delimiters (`}`, `);`, `]`): a
/// declaration head's line range already gives where it ends.
pub(super) fn drop_closing_rows(lines: &[&str], keep: &mut [bool]) {
    for (row, line) in lines.iter().enumerate() {
        let text = line.trim();
        if !text.is_empty()
            && text
                .chars()
                .all(|c| matches!(c, '}' | ')' | ']' | ';' | ','))
        {
            keep[row] = false;
        }
    }
}

/// The text a docstring's first row holds after its opening quotes.
fn docstring_opening_text(row: &str) -> &str {
    let row = row
        .trim_start()
        .trim_start_matches(|c: char| c.is_ascii_alphabetic());
    for quote in ["\"\"\"", "\'\'\'", "\"", "\'"] {
        if let Some(text) = row.strip_prefix(quote) {
            return text.trim();
        }
    }
    row.trim()
}

/// Cut a multi-row module or class docstring to its first text row.
pub(super) fn collapse_docstrings(
    root: tree_sitter::Node<'_>,
    lines: &[&str],
    keep: &mut [bool],
    elided: &mut [bool],
) {
    let mut scopes = vec![root];
    while let Some(scope) = scopes.pop() {
        let body = if scope.kind() == "class_definition" {
            scope.child_by_field_name("body")
        } else {
            Some(scope)
        };
        let Some(body) = body else { continue };
        let mut cursor = body.walk();
        let mut first = true;
        for child in body.named_children(&mut cursor) {
            if child.kind() == "comment" {
                continue;
            }
            let docstring = first
                && child.kind() == "expression_statement"
                && child.named_child_count() == 1
                && child.named_child(0).is_some_and(|c| c.kind() == "string");
            first = false;
            if docstring {
                let (start, end) = (child.start_position().row, child.end_position().row);
                if start < end && end < lines.len() {
                    let shown = if docstring_opening_text(lines[start]).is_empty() {
                        (start + 1..=end).find(|row| !lines[*row].trim().is_empty())
                    } else {
                        Some(start)
                    };
                    keep[start..=end].fill(false);
                    keep[start] = true;
                    if let Some(shown) = shown {
                        keep[shown] = true;
                        // The closing row carries its own quotes: not a cut.
                        elided[shown] = shown < end;
                    }
                }
            } else if matches!(child.kind(), "class_definition" | "decorated_definition") {
                let class = if child.kind() == "class_definition" {
                    Some(child)
                } else {
                    child
                        .child_by_field_name("definition")
                        .filter(|d| d.kind() == "class_definition")
                };
                scopes.extend(class);
            }
        }
    }
}
