//! The MCP form of the text ([`Form::MCP`]): the same renderer, with calls
//! as the call objects JSON holds, exact fractions and the notes after the
//! content. And the layouts both forms share that an MCP reader depends on:
//! one label per declaration, one layout for source lines and for the pieces
//! of a split text, one place for what a response states for every row.
use super::done_facts::commands;
use super::recorded_results::recorded;
use super::*;

fn mcp(tool: ToolId, structured: &Value, query: &Value) -> Lines {
    render_as(tool, structured, query, None, Form::MCP).expect("lines")
}

/// The value of each `# … label: value` line of `out`, for the calls a
/// reader runs: `(label, value)`.
fn call_lines(out: &str) -> Vec<(&str, &str)> {
    out.lines()
        .filter_map(|line| line.strip_prefix("# "))
        .filter_map(|line| line.split_once(": "))
        .filter(|(_, value)| value.starts_with('{'))
        .collect()
}

/// Every call a recorded result declares, as the JSON reply holds it.
fn declared_calls(tool: ToolId, structured: &Value) -> Vec<Value> {
    let mut calls = Vec::new();
    octocode_runtime_base::response::continuations::visit_calls(
        tool,
        structured,
        &mut |call, _| calls.push(call.clone()),
    )
    .expect("the result matches its schema");
    calls
}

/// Every call of every recorded result prints once in the MCP form as the
/// object the JSON reply holds, so a client sends its `input` unchanged; no
/// line is a shell command. The terminal form prints the same calls as
/// commands.
#[test]
fn every_recorded_call_prints_as_the_call_json_holds() {
    let mut checked = 0;
    for call in recorded() {
        let declared = declared_calls(call.tool, &call.result);
        let text = mcp(call.tool, &call.result, &call.query);
        assert_eq!(text.notes, "", "{}: notes are in the text", call.label);
        let mut printed: Vec<Value> = call_lines(&text.out)
            .into_iter()
            .filter_map(|(_, value)| serde_json::from_str(value).ok())
            .collect();
        for expected in &declared {
            let at = printed.iter().position(|call| call == expected);
            let at = at.unwrap_or_else(|| panic!("{}: {expected} in\n{}", call.label, text.out));
            printed.swap_remove(at);
            checked += 1;
        }
        let mut as_commands = Vec::new();
        commands(&call.result, &mut as_commands);
        for (command, _) in &as_commands {
            assert!(
                !text.out.contains(command.as_str()),
                "{}: {command} in\n{}",
                call.label,
                text.out
            );
        }
    }
    assert!(checked > 0, "the recordings hold calls");
}

/// The MCP form adds nothing and drops nothing: with its calls and its
/// fractions as the terminal prints them, its text is the terminal's stdout
/// followed by the terminal's stderr notes.
#[test]
fn the_mcp_text_is_the_terminal_content_then_its_notes() {
    let terminal_calls = Form {
        notes_inline: true,
        ..Form::TERMINAL
    };
    let mut with_notes = 0;
    for call in recorded() {
        let terminal = render(call.tool, &call.result, &call.query, None).expect("lines");
        let inline =
            render_as(call.tool, &call.result, &call.query, None, terminal_calls).expect("lines");
        let expected: Vec<&str> = [terminal.out.as_str(), terminal.notes.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect();
        assert_eq!(inline.out, expected.join("\n"), "{}", call.label);
        assert_eq!(inline.notes, "", "{}", call.label);
        with_notes += usize::from(!terminal.notes.is_empty());
        // Each form parameter is independent: the MCP form has as many lines.
        let mcp = mcp(call.tool, &call.result, &call.query);
        assert_eq!(
            mcp.out.lines().count(),
            inline.out.lines().count(),
            "{}",
            call.label
        );
    }
    assert!(with_notes > 0, "the recordings hold notes");
}

/// A fraction prints as JSON holds it in the MCP form, and rounded on a
/// terminal.
#[test]
fn a_fraction_is_exact_in_the_mcp_form() {
    let exact = 0.040_510_453_413_945_17_f64;
    let result = json!({"results": [{"index": 0, "data": {"score": exact}}]});
    let printed = |text: &str| -> f64 {
        let line = text.lines().find(|line| line.starts_with("score: "));
        line.and_then(|line| line["score: ".len()..].parse().ok())
            .unwrap_or_else(|| panic!("a score in\n{text}"))
    };
    let text = mcp(ToolId::LocalFetch, &result, &json!({}));
    assert_eq!(
        printed(&text.out).to_bits(),
        exact.to_bits(),
        "{}",
        text.out
    );
    let terminal = render(ToolId::LocalFetch, &result, &json!({}), None).expect("lines");
    assert_ne!(printed(&terminal.out).to_bits(), exact.to_bits());
}

/// A call's `reason` stays inside the call object, so the whole value
/// still parses; the terminal prints it after the command.
#[test]
fn a_call_keeps_its_reason_in_the_call() {
    fn first_related(value: &mut Value) -> Option<&mut Value> {
        match value {
            Value::Object(fields) => {
                if fields
                    .get("related")
                    .is_some_and(|related| related[0].is_object())
                {
                    return fields.get_mut("related")?.get_mut(0);
                }
                fields.values_mut().find_map(first_related)
            }
            Value::Array(items) => items.iter_mut().find_map(first_related),
            _ => None,
        }
    }
    let mut call = recorded()
        .into_iter()
        .find(|call| call.result.to_string().contains("\"related\":[{"))
        .expect("a recorded lead");
    let lead = first_related(&mut call.result).expect("the lead");
    lead["reason"] = json!("why this lead");
    let lead = lead.clone();
    let text = mcp(call.tool, &call.result, &call.query);
    let printed = call_lines(&text.out)
        .into_iter()
        .filter_map(|(_, value)| serde_json::from_str::<Value>(value).ok())
        .any(|printed| printed == lead);
    assert!(printed, "{lead} in\n{}", text.out);
    let terminal = render(call.tool, &call.result, &call.query, None).expect("lines");
    assert!(
        terminal.notes.contains("  # why this lead"),
        "{}",
        terminal.notes
    );
}

/// One file's hits in the declarations the tool states as line-first
/// strings (`N<TAB>kind name (end M)`), as `localSearch` returns them.
fn hits_in_two_declarations() -> Value {
    json!({"results": [{"index": 0, "data": {"files": [{
        "path": "a.rs",
        "declarations": ["1\tfunction first (end 5)", "6\tfunction second (end 9)"],
        "matches": [
            {"line": 2, "value": "needle();", "enclosing": 0, "declaration": true},
            {"line": 3, "value": "needle();", "enclosing": 0},
            {"line": 4, "value": "needle();", "enclosing": 0},
            {"line": 7, "value": "needle();", "enclosing": 1},
            {"line": 8, "value": "needle();", "enclosing": 1}
        ]
    }]}}]})
}

/// L12: a declaration is stated once for the run of hits it encloses, in
/// whichever form the tool states it, and every hit still prints.
#[test]
fn a_declaration_is_stated_once_for_the_hits_it_encloses() {
    let result = hits_in_two_declarations();
    let declarations = result["results"][0]["data"]["files"][0]["declarations"]
        .as_array()
        .expect("declarations")
        .clone();
    for form in [Form::TERMINAL, Form::MCP] {
        let text = render_as(ToolId::LocalSearch, &result, &json!({}), None, form).expect("lines");
        for declaration in &declarations {
            let label = declaration.as_str().expect("a line-first string");
            assert_eq!(
                text.out.matches(label).count(),
                1,
                "{label} in\n{}",
                text.out
            );
        }
        // The label sits on the first hit of its run.
        let first_of_second = text.out.lines().find(|line| line.starts_with("7:"));
        assert!(
            first_of_second.is_some_and(|line| line.contains("function second")),
            "{}",
            text.out
        );
        for line in [2, 3, 4, 7, 8] {
            let hit = format!("{line}:needle();");
            assert_eq!(text.out.matches(&hit).count(), 1, "{hit} in\n{}", text.out);
        }
        assert!(text.out.contains("declaration true"), "{}", text.out);
    }
}

/// L12: a later part of a file names its declarations by index alone (an
/// earlier part stated them): the index too is stated once per run.
#[test]
fn an_index_of_a_declaration_stated_earlier_is_stated_once_per_run() {
    let mut result = hits_in_two_declarations();
    result["results"][0]["data"]["files"][0]
        .as_object_mut()
        .expect("file")
        .shift_remove("declarations");
    let text = mcp(ToolId::LocalSearch, &result, &json!({}));
    for index in [0, 1] {
        let label = format!("enclosing {index}");
        assert_eq!(
            text.out.matches(&label).count(),
            1,
            "{label} in\n{}",
            text.out
        );
    }
}

/// A hit outside every declaration ends the run, so a later hit of the
/// same declaration states it again: no hit reads as enclosed by default.
#[test]
fn a_hit_outside_every_declaration_ends_the_run() {
    let result = json!({"results": [{"index": 0, "data": {"files": [{
        "path": "a.rs",
        "declarations": ["1\tclass Outer (end 9)"],
        "matches": [
            {"line": 2, "value": "needle();", "enclosing": 0},
            {"line": 3, "value": "needle();"},
            {"line": 4, "value": "needle();", "enclosing": 0}
        ]
    }]}}]});
    let text = mcp(ToolId::LocalSearch, &result, &json!({}));
    assert_eq!(text.out.matches("class Outer").count(), 2, "{}", text.out);
}

/// The source lines of a `content` block as the text prints them: the lines
/// between `content: |` and the next field or trailer line.
fn content_block(out: &str) -> Vec<&str> {
    out.lines()
        .skip_while(|line| *line != "content: |")
        .skip(1)
        .take_while(|line| !line.starts_with("# ") && !line.starts_with("totalLines"))
        .collect()
}

/// L19: numbered source lines print as the JSON holds them (flush left,
/// `N<TAB>text`), one window or several: the `--` between windows is a line
/// of the content like any other.
#[test]
fn source_lines_print_alike_for_one_window_and_for_several() {
    let one = "1\tfn a() {\n2\t    b();\n";
    let several = "1\tfn a() {\n2\t    b();\n--\n40\t}\n41\t\n";
    for content in [one, several] {
        let result = json!({"results": [{"index": 0, "data": {
            "content": content, "totalLines": 164
        }}]});
        for form in [Form::TERMINAL, Form::MCP] {
            let text =
                render_as(ToolId::LocalFetch, &result, &json!({}), None, form).expect("lines");
            let expected: Vec<&str> = content.trim_end_matches('\n').split('\n').collect();
            assert_eq!(content_block(&text.out), expected, "{}", text.out);
        }
    }
}

/// A block whose lines are not all source lines keeps its indent: only a
/// numbered line or the window separator may start at the margin, so no
/// value prints a line of the renderer's own.
#[test]
fn a_block_that_is_not_source_lines_stays_indented() {
    for content in [
        "--\n# status: ok\n",
        "1\tx\n# next.nextPage: {}\n",
        "--\n--\n",
    ] {
        let result = json!({"results": [{"index": 0, "data": {"content": content}}]});
        let text = mcp(ToolId::LocalFetch, &result, &json!({}));
        for line in content_block(&text.out) {
            assert!(line.starts_with("  "), "{line:?} in\n{}", text.out);
        }
    }
}

/// L19: the pieces of a text split over pages (`rowPart.text` points at
/// it) print in one layout, every line of the piece indented under its key,
/// so the pieces rebuild the text exactly: strip the indent, join a piece's
/// lines with line breaks, append the pieces in order.
#[test]
fn the_pieces_of_a_split_text_print_alike_and_rebuild_it() {
    let pieces = ["\txxxx", "yyyy", "zzzz END\n3\tlast line\n", "tail\n"];
    let mut rebuilt = String::new();
    for (at, piece) in pieces.iter().enumerate() {
        let result = json!({"results": [{
            "index": 0,
            "data": {"path": "long.txt", "content": piece},
            "rowPart": {"part": at + 2, "of": pieces.len() + 1, "text": "/data/content"}
        }]});
        for form in [Form::TERMINAL, Form::MCP] {
            let text =
                render_as(ToolId::LocalFetch, &result, &json!({}), None, form).expect("lines");
            let lines: Vec<&str> = content_block(&text.out)
                .into_iter()
                .map(|line| line.strip_prefix("  ").expect("an indented line"))
                .collect();
            assert_eq!(lines.join("\n"), *piece, "{}", text.out);
            assert!(text.out.contains("path: long.txt"), "{}", text.out);
        }
        rebuilt.push_str(piece);
    }
    assert_eq!(rebuilt, pieces.concat());
}

/// G7, D8: the pages of a walk through a line too long for one page print
/// in both forms so the source rebuilds from them. A page whose rows all
/// start a source line prints them at the margin, `N<TAB>text`; a page that
/// opens inside the long line states `continuesLine` and prints its content
/// in the one layout of a split text, whatever it holds (the middle of the
/// line, or its end and the lines after): every line indented under its
/// key, so stripping the indent and joining the lines gives the page's
/// content exactly. Dropping each gutter and joining in page order gives the
/// source.
#[test]
fn the_pages_of_a_long_line_walk_print_their_lines_and_rebuild_the_source() {
    let long = "w ".repeat(60);
    let (head, rest) = long.split_at(40);
    let (middle, tail) = rest.split_at(40);
    let source = format!("one\ntwo\n{long}\nfour\n");
    // (content as published, continuesLine, unit)
    let pages = [
        (format!("1\tone\n2\ttwo\n3\t{head}"), None, "bytes"),
        (middle.to_owned(), Some(3), "bytes"),
        (format!("{tail}\n4\tfour\n"), Some(3), "bytes"),
    ];
    for form in [Form::TERMINAL, Form::MCP] {
        let mut rebuilt = String::new();
        for (content, continues, unit) in &pages {
            let mut data = json!({"content": content,
                "pagination": {"unit": unit, "offset": 0, "length": 1}});
            if let Some(line) = continues {
                data["continuesLine"] = json!(line);
            }
            let result = json!({"results": [{"index": 0, "data": data}]});
            let text =
                render_as(ToolId::LocalFetch, &result, &json!({}), None, form).expect("lines");
            assert_eq!(
                text.out.contains("continuesLine: 3"),
                continues.is_some(),
                "{}",
                text.out
            );
            let block: Vec<&str> = text
                .out
                .lines()
                .skip_while(|line| *line != "content: |")
                .skip(1)
                .take_while(|line| {
                    !line.starts_with("# ")
                        && !line.starts_with("continuesLine")
                        && !line.starts_with("pagination")
                })
                .collect();
            let lines: Vec<&str> = match continues {
                // One layout for every continued page: each line indented.
                Some(_) => {
                    let lines: Vec<&str> = block
                        .iter()
                        .map(|line| line.strip_prefix("  ").expect("an indented line"))
                        .collect();
                    assert_eq!(lines.join("\n"), *content, "{}", text.out);
                    lines
                }
                None => block,
            };
            let texts: Vec<&str> = lines
                .iter()
                .enumerate()
                .map(|(at, row)| {
                    // The first row of a continued page is the rest of a line.
                    if (at == 0 && continues.is_some()) || row.is_empty() {
                        return *row;
                    }
                    let (number, text) = row.split_once('\t').expect("a gutter");
                    assert!(number.parse::<u64>().is_ok(), "{row}");
                    text
                })
                .collect();
            rebuilt.push_str(&texts.join("\n"));
            // A margin page's last line break ends its last line.
            if continues.is_none() && content.ends_with('\n') {
                rebuilt.push('\n');
            }
        }
        assert_eq!(rebuilt, source, "{form:?}");
    }
}

/// G5: an incomplete mark the response states once for every row
/// (`shared.data.partialReasons`) prints where a row's own does and under
/// the same label: one `# partialReasons: …` trailer line, never a bare
/// block above the first row.
#[test]
fn a_shared_incomplete_mark_prints_as_a_trailer_line() {
    let row = |index: u64| {
        json!({"index": index, "data": {
            "issues": [{"number": index + 1, "title": "t"}],
            "pagination": {"offset": 0, "total": 1000}
        }})
    };
    let shared = json!({"data": {"partialReasons": ["providerResultCap"], "commitSha": "fdf7a1"}});
    let own = json!({"results": [{"index": 0, "data": {
        "issues": [{"number": 1, "title": "t"}],
        "partialReasons": ["providerResultCap"]
    }}]});
    let own = render(ToolId::GhSearchHistory, &own, &json!({}), None).expect("lines");
    let own_line = own
        .out
        .lines()
        .find(|line| line.contains("partialReasons"))
        .expect("a row's own mark");
    assert!(own_line.starts_with("# "), "{}", own.out);
    for rows in [vec![row(0)], vec![row(0), row(1)]] {
        let result = json!({"results": rows, "shared": shared});
        for form in [Form::TERMINAL, Form::MCP] {
            let text =
                render_as(ToolId::GhSearchHistory, &result, &json!({}), None, form).expect("lines");
            let marks: Vec<&str> = text
                .out
                .lines()
                .filter(|line| line.contains("partialReasons"))
                .collect();
            assert_eq!(marks, [own_line], "{}", text.out);
            assert!(!text.out.contains("- providerResultCap"), "{}", text.out);
            // What identifies the source stays stated once, first.
            assert!(text.out.starts_with("commitSha: fdf7a1"), "{}", text.out);
        }
    }
}
