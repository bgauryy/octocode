use super::*;

/// Naive per-call scan from the line start — the pre-checkpoint behavior.
fn naive_position(content: &str, byte: usize) -> (u32, u32) {
    let byte = content.floor_char_boundary(byte);
    let line_start = content[..byte].rfind('\n').map_or(0, |nl| nl + 1);
    let line = content[..line_start].matches('\n').count() as u32;
    let mut column: u32 = content[line_start..byte]
        .chars()
        .map(|c| c.len_utf16() as u32)
        .sum();
    if line == 0 && content.starts_with(BOM) {
        column = column.saturating_sub(1);
    }
    (line, column)
}

fn long_mixed_content() -> String {
    let mut content = String::from(BOM);
    for i in 0..6_000 {
        content.push_str(match i % 7 {
            0 => "é",
            1 => "😀",
            2 => "\r\n",
            3 => "abc",
            4 => "\n",
            5 => "中",
            _ => "x",
        });
        if i % 997 == 0 {
            content.push_str(&"z".repeat(3_000)); // very long line runs
        }
    }
    content
}

#[test]
fn checkpointed_columns_match_a_naive_scan_everywhere() {
    let content = long_mixed_content();
    let index = LineIndex::tree_sitter(&content);
    for (byte, _) in content.char_indices().step_by(7) {
        assert_eq!(
            index.byte_to_position(&content, byte as u32),
            naive_position(&content, byte),
            "byte {byte}"
        );
        if byte == 0 {
            continue; // the hidden BOM maps back to the first byte after it
        }
        let (line, column) = index.byte_to_position(&content, byte as u32);
        // position_to_byte inverts byte_to_position on char boundaries.
        assert_eq!(
            index.position_to_byte(&content, line, column) as usize,
            byte,
            "round trip at byte {byte}"
        );
    }
}

#[test]
fn a_minified_single_line_stays_linear() {
    // 2 MB on one line with multibyte chars: a per-call line scan would
    // make 20k lookups ~quadratic; checkpoints keep it to one gap per lookup.
    let content = "é😀ab".repeat(250_000);
    let index = LineIndex::tree_sitter(&content);
    let started = std::time::Instant::now();
    let mut total = 0u64;
    for (byte, _) in content.char_indices().step_by(50) {
        total += u64::from(index.byte_to_position(&content, byte as u32).1);
    }
    assert!(total > 0);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "took {:?}",
        started.elapsed()
    );
}
