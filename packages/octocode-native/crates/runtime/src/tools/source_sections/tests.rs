use super::*;

fn plan(source: &str, path: &Path, selected: &[Range<usize>], target_chars: usize) -> Vec<Section> {
    let mut sections = Vec::new();
    let complete = visit(source, path, selected, target_chars, |section| {
        sections.push(section);
        Ok::<_, ()>(true)
    });
    assert_eq!(complete, Ok(true));
    sections
}

fn line_range(source: &str, bytes: &Range<usize>) -> [usize; 2] {
    let first = source.as_bytes()[..bytes.start]
        .iter()
        .filter(|&&byte| byte == b'\n')
        .count()
        + 1;
    let last_byte = bytes.end.saturating_sub(1).max(bytes.start);
    let last = source.as_bytes()[..last_byte]
        .iter()
        .filter(|&&byte| byte == b'\n')
        .count()
        + 1;
    [first, last]
}

#[test]
fn exported_declarations_keep_wrapper_syntax_and_exact_coverage() {
    let source = "export async function first(){return '🙂';}\nexport default class Second {}\nexport interface Third { value: string }\n";
    let path = Path::new("exports.ts");
    let selected = 0..source.len();
    let sections = plan(source, path, std::slice::from_ref(&selected), 200);
    assert_eq!(sections.len(), 3);
    assert_eq!(
        sections
            .iter()
            .filter_map(|s| s.name.as_deref())
            .collect::<Vec<_>>(),
        vec!["first", "Second", "Third"]
    );
    assert!(
        sections
            .iter()
            .all(|s| source[s.target.clone()].starts_with("export "))
    );
    for budget in 1..=200 {
        let sections = plan(source, path, std::slice::from_ref(&selected), budget);
        assert_eq!(
            sections
                .iter()
                .flat_map(|s| s.target.clone())
                .collect::<Vec<_>>(),
            (0..source.len()).collect::<Vec<_>>()
        );
        assert!(
            sections
                .iter()
                .all(|s| source[s.target.clone()].chars().count() <= budget)
        );
    }
}

#[test]
fn partitions_every_selected_byte_across_scripts_and_line_endings() {
    for text in [
        "αβγ🙂中文\r\nمرحبا\nשלום",
        "without whitespace🙂🙂🙂",
        "a\n\nb\n\nlast",
        "\u{feff}a\r\nb\r\n",
    ] {
        for budget in 1..=text.chars().count() {
            let plan = plan(
                text,
                Path::new("unknown.xyz"),
                std::slice::from_ref(&(0..text.len())),
                budget,
            );
            let mut next = 0;
            for section in plan {
                assert_eq!(section.target.start, next);
                assert!(section.target.start < section.target.end);
                assert!(text.is_char_boundary(section.target.end));
                assert!(text[section.target.clone()].chars().count() <= budget);
                assert!(
                    section
                        .context
                        .iter()
                        .all(|context| context.end <= section.target.start
                            || context.start >= section.target.end)
                );
                next = section.target.end;
            }
            assert_eq!(next, text.len());
        }
    }
}

#[test]
fn disjoint_selection_keeps_original_offsets_and_no_gap_target() {
    let text = "one\ntwo\nthree\nfour\n";
    let selected = [0..4, 8..text.len()];
    let plan = plan(text, Path::new("text.txt"), &selected, 5);
    let actual: Vec<_> = plan
        .iter()
        .flat_map(|section| section.target.clone())
        .collect();
    let expected: Vec<_> = selected.iter().flat_map(Clone::clone).collect();
    assert_eq!(actual, expected);
    assert_eq!(line_range(text, &(8..14)), [3, 3]);
}

#[test]
fn declaration_documentation_and_utf16_geometry_stay_with_the_target() {
    let source = "\u{feff}// 🙂 prefix\r\n/** חוזה 中文 */\r\nfunction checked(){return '🙂';}\r\n";
    let sections = plan(
        source,
        Path::new("documented.ts"),
        std::slice::from_ref(&(0..source.len())),
        100,
    );
    let declaration = sections
        .iter()
        .find(|section| section.name.as_deref() == Some("checked"))
        .expect("documented declaration");
    assert_eq!(
        &source[declaration.target.clone()],
        "/** חוזה 中文 */\r\nfunction checked(){return '🙂';}\r\n"
    );
    assert_eq!(line_range(source, &declaration.target), [2, 3]);
    assert_eq!(
        sections
            .iter()
            .map(|section| &source[section.target.clone()])
            .collect::<String>(),
        source
    );
}

#[test]
fn traversal_stops_before_materializing_unconsumed_sections() {
    let source = (0..500)
        .map(|index| format!("function f{index}(){{return {index};}}\n"))
        .collect::<String>();
    let mut visited = 0;
    let completed = visit(
        &source,
        Path::new("many.ts"),
        std::slice::from_ref(&(0..source.len())),
        50,
        |_| {
            visited += 1;
            Ok::<_, ()>(visited < 3)
        },
    );
    assert_eq!(completed, Ok(false));
    assert_eq!(visited, 3);
}

#[test]
fn whitespace_attaches_without_merging_declarations_or_losing_documentation() {
    let source = " \r\n\tfunction first(){return '🙂';}\r\n \u{2003}\t\n  /** second 中文 */\r\n  function second(){return 'עברית';}\r\n\t ";
    let sections = plan(
        source,
        Path::new("whitespace.ts"),
        std::slice::from_ref(&(0..source.len())),
        200,
    );
    assert_eq!(sections.len(), 2);
    assert_eq!(
        sections
            .iter()
            .filter_map(|s| s.name.as_deref())
            .collect::<Vec<_>>(),
        vec!["first", "second"]
    );
    assert!(sections.iter().all(|s| s.kind == "function"));
    assert!(source[sections[1].target.clone()].contains("/** second 中文 */"));
    assert_eq!(sections[0].target.start, 0);
    assert_eq!(sections[0].target.end, sections[1].target.start);
    assert_eq!(sections[1].target.end, source.len());
    assert_eq!(
        sections
            .iter()
            .map(|s| &source[s.target.clone()])
            .collect::<String>(),
        source
    );
}

#[test]
fn whitespace_attachment_respects_every_target_budget_and_context_boundary() {
    let source = " \tfunction alpha(){return '🙂';}\r\n\u{2003}\nfunction beta(){return '中';}\n\t";
    for budget in 1..=source.chars().count() {
        let sections = plan(
            source,
            Path::new("bounded.ts"),
            std::slice::from_ref(&(0..source.len())),
            budget,
        );
        let mut end = 0;
        for section in &sections {
            assert_eq!(section.target.start, end);
            assert!(section.target.end > end);
            assert!(source[section.target.clone()].chars().count() <= budget);
            assert!(section.context.iter().all(|context| {
                context.end <= section.target.start || context.start >= section.target.end
            }));
            if let Some(name) = section.name.as_deref() {
                let target = &source[section.target.clone()];
                assert!(target.contains(&format!("function {name}()")));
                assert_eq!(target.matches("function ").count(), 1);
            }
            end = section.target.end;
        }
        assert_eq!(end, source.len());
    }
}

#[test]
fn whitespace_attachment_stops_at_substantive_text_and_selection_boundaries() {
    let source = "function one(){return 1;}\n  console.log('substantive');\n function two(){return 2;}\nEXCLUDED\n \tfunction three(){return 3;}\n";
    let gap = source.find("EXCLUDED").expect("excluded marker");
    let resumed = gap + "EXCLUDED\n".len();
    let selected = [0..gap, resumed..source.len()];
    let sections = plan(source, Path::new("selections.ts"), &selected, 200);
    assert_eq!(
        sections
            .iter()
            .filter_map(|s| s.name.as_deref())
            .collect::<Vec<_>>(),
        vec!["one", "two", "three"]
    );
    assert!(sections.iter().any(
        |s| s.name.is_none() && source[s.target.clone()].contains("console.log('substantive')")
    ));
    assert!(sections.iter().all(|s| {
        selected
            .iter()
            .any(|range| range.start <= s.target.start && s.target.end <= range.end)
    }));
    assert_eq!(
        sections
            .iter()
            .map(|s| &source[s.target.clone()])
            .collect::<String>(),
        format!("{}{}", &source[..gap], &source[resumed..])
    );
}

/// Attribute lines and a container's closing brace are no paid section
/// of their own: they travel with the declaration they belong to.
#[test]
fn attributes_and_closing_braces_stay_with_their_declarations() {
    let source = "fn top() { let value = 1; }\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn a() { assert!(true); }\n\n    #[test]\n    fn b() { assert!(true); }\n}\n";
    let selected = 0..source.len();
    let sections = plan(
        source,
        Path::new("leads.rs"),
        std::slice::from_ref(&selected),
        60,
    );
    let targets: Vec<&str> = sections
        .iter()
        .map(|section| source[section.target.clone()].trim())
        .collect();
    for trivia in ["#[cfg(test)]", "#[test]", "}", ""] {
        assert!(
            !targets.contains(&trivia),
            "standalone {trivia:?} in {targets:?}"
        );
    }
    assert_eq!(
        sections
            .iter()
            .map(|section| &source[section.target.clone()])
            .collect::<String>(),
        source
    );
    assert!(
        sections
            .iter()
            .all(|section| source[section.target.clone()].chars().count() <= 60)
    );
}

#[test]
fn minified_declarations_use_byte_ranges_after_multibyte_text() {
    let text = "const title='🙂中文';function first(){return title;}function second(){return 2;}";
    let plan = plan(
        text,
        Path::new("fixture.ts"),
        std::slice::from_ref(&(0..text.len())),
        40,
    );
    let first = plan
        .iter()
        .find(|section| section.name.as_deref() == Some("first"))
        .expect("first declaration");
    assert_eq!(
        &text[first.target.clone()],
        "function first(){return title;}"
    );
    assert_eq!(line_range(text, &first.target), [1, 1]);
    assert_eq!(
        plan.iter()
            .map(|section| &text[section.target.clone()])
            .collect::<String>(),
        text
    );
}
