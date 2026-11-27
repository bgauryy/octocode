use super::SEPARATOR;

/// One gutter for every outline: each `N<space>text`, `N\ttext` or `N}` line
/// becomes `N\ttext`. Lines without a leading number stay as they are.
#[must_use]
pub fn tab_gutter(outline: &str) -> String {
    let mut output = String::with_capacity(outline.len());
    for record in outline.split_inclusive('\n') {
        let digits = record.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 || record.starts_with('0') || digits == record.trim_end().len() {
            output.push_str(record);
            continue;
        }
        let (number, rest) = record.split_at(digits);
        output.push_str(number);
        output.push(SEPARATOR);
        output.push_str(rest.strip_prefix([' ', SEPARATOR]).unwrap_or(rest));
    }
    output
}
