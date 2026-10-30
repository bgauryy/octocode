use crate::response::numbered::{GAP, line_faithful_view, number_view};

/// Transform with the full lexical context, then project requested source lines.
/// A selected range may begin inside a literal whose delimiter is outside it.
pub(super) fn render(
    full_source: &str,
    selected: &str,
    path: &str,
    source_lines: Option<&[usize]>,
) -> String {
    let source = if source_lines.is_some() {
        full_source
    } else {
        selected
    };
    let view = octocode_engine::minify::apply::apply_content_view_minification(source, path);
    let numbered =
        number_view(source, None, &view).unwrap_or_else(|| line_faithful_view(source, None));
    let Some(wanted) = source_lines else {
        return numbered;
    };
    let mut records = numbered
        .split_inclusive('\n')
        .filter_map(|record| {
            let (number, _) = record.split_once('\t')?;
            Some((number.parse::<usize>().ok()?, record))
        })
        .peekable();
    let mut output = String::new();
    for &number in wanted {
        if number == 0 {
            output.push_str(GAP);
            continue;
        }
        while records.peek().is_some_and(|(line, _)| *line < number) {
            records.next();
        }
        while records.peek().is_some_and(|(line, _)| *line == number) {
            if let Some((_, record)) = records.next() {
                output.push_str(record);
            }
        }
    }
    output
}
