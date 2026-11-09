use super::*;

fn grouped(
    source: &str,
    path: &Path,
    selected: &[Range<usize>],
    target_chars: usize,
) -> Vec<Section> {
    let mut sections = Vec::new();
    let complete = visit_grouped(source, path, selected, target_chars, |section| {
        sections.push(section);
        Ok::<_, ()>(true)
    });
    assert_eq!(complete, Ok(true));
    sections
}

#[test]
fn grouped_pages_join_small_declarations_and_keep_exact_coverage() {
    let source = (0..40)
        .map(|index| format!("pub struct Item{index} {{ pub value: u32 }}\n"))
        .collect::<String>();
    let path = Path::new("items.rs");
    let selected = 0..source.len();
    let single = plan(&source, path, std::slice::from_ref(&selected), 1_600);
    let pages = grouped(&source, path, std::slice::from_ref(&selected), 1_600);
    assert_eq!(single.len(), 40);
    assert!(pages.len() < single.len() / 5, "{} pages", pages.len());
    let mut offset = 0;
    for page in &pages {
        assert_eq!(page.target.start, offset);
        assert!(source[page.target.clone()].chars().count() <= 1_600 / 4);
        offset = page.target.end;
    }
    assert_eq!(offset, source.len());
    assert_eq!(pages[0].kind, grouped::GROUP_KIND);
    assert_eq!(pages[0].context, single[0].context);
    // A merged page is named by its first and last member and how many it
    // joins, not by every member (B51 11a). Its lines are the section's
    // `target.lines`, beside the name, so the name does not repeat them.
    let mut first = 0;
    for page in &pages {
        let members = source[page.target.clone()].lines().count();
        let last = first + members - 1;
        let count = format!("{members} declarations");
        let expected = match members {
            1 => format!("Item{first}"),
            2 => format!("Item{first}, Item{last} ({count})"),
            _ => format!("Item{first} … Item{last} ({count})"),
        };
        assert_eq!(page.name.as_deref(), Some(expected.as_str()));
        first = last + 1;
    }
    assert_eq!(first, 40);
}

/// Declaration names repeat (the same items in two places, or members with
/// no name): a merged page's name states how many sections it joins, named
/// or not (R10-5). Its id and target lines tell pages apart, so the name
/// carries no line span of its own.
#[test]
fn grouped_page_names_are_distinct_and_count_every_member() {
    let half = (0..20)
        .map(|index| {
            format!("pub struct Item{index} {{ pub value: u32 }}\nmacro_call!({index});\n")
        })
        .collect::<String>();
    let source = format!("{half}{half}");
    let path = Path::new("items.rs");
    let selected = 0..source.len();
    let single = plan(&source, path, std::slice::from_ref(&selected), 1_600);
    let pages = grouped(&source, path, std::slice::from_ref(&selected), 1_600);
    let mut names = std::collections::HashSet::new();
    for page in &pages {
        let members = single
            .iter()
            .filter(|section| {
                page.target.start <= section.target.start && section.target.end <= page.target.end
            })
            .count();
        if members < 2 {
            continue;
        }
        let name = page.name.as_deref().expect("a merged page has a name");
        assert!(
            name.ends_with(&format!("({members} declarations)")),
            "{name}: {members} members"
        );
        let [start, end] = line_range(&source, &page.target);
        assert!(names.insert([start, end]), "{name}: pages never overlap");
    }
    assert!(names.len() > 1, "{} merged pages", names.len());
}

#[test]
fn grouped_pages_keep_a_lone_section_and_disjoint_selections_apart() {
    let source = "fn alpha() { 1 }\nfn beta() { 2 }\n";
    let path = Path::new("pair.rs");
    let first = 0..source.find("fn beta").unwrap();
    let pages = grouped(
        source,
        path,
        &[first.clone(), first.end + 3..source.len()],
        400,
    );
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0].name.as_deref(), Some("alpha"));
    assert_ne!(pages[0].kind, grouped::GROUP_KIND);
}

#[test]
fn grouped_traversal_stops_when_the_consumer_stops() {
    let source = (0..40)
        .map(|index| format!("pub struct Item{index} {{ pub value: u32 }}\n"))
        .collect::<String>();
    let mut seen = 0;
    let selected = 0..source.len();
    let complete = visit_grouped(
        &source,
        Path::new("items.rs"),
        std::slice::from_ref(&selected),
        200,
        |_| {
            seen += 1;
            Ok::<_, ()>(seen < 2)
        },
    );
    assert_eq!((complete, seen), (Ok(false), 2));
}

/// A file that fits one section is not one page: its declarations still
/// split, so an answer's lines narrow what is worth reading.
#[test]
fn a_file_within_one_section_still_splits_into_pages_and_keeps_exact_coverage() {
    let source = (0..12)
        .map(|index| {
            format!(
                "fn function_{index}() {{\n{}}}\n\n",
                "    let value = compute();\n".repeat(16)
            )
        })
        .collect::<String>();
    const SECTION: usize = 8_000;
    assert!(
        source.chars().count() < SECTION,
        "the file fits one section"
    );
    let selected = 0..source.len();
    let pages = grouped(
        &source,
        Path::new("small.rs"),
        std::slice::from_ref(&selected),
        SECTION,
    );
    assert!(pages.len() > 1, "{} page for the whole file", pages.len());
    let mut offset = 0;
    for page in &pages {
        assert_eq!(page.target.start, offset);
        offset = page.target.end;
    }
    assert_eq!(offset, source.len());
}
