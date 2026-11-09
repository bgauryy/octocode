//! A long line whose walk outgrew the memory budget says every value it shows is hidden.

use super::*;

/// A line of Kubernetes Secret objects, each value fake, longer than a whole-file read.
fn secret_line() -> String {
    let mut line = String::from(r#"{"items":["#);
    let mut index = 0;
    while line.len() < LONG_LINE_BYTES {
        line.push_str(&format!(
            r#"{{"data":{{"k":"v{index:06}"}},"kind":"Secret"}},"#
        ));
        index += 1;
    }
    line.push_str("{\"kind\":\"ConfigMap\"}]}");
    line
}

#[test]
fn a_walk_over_its_budget_is_reported_as_every_value() {
    let line = secret_line();
    let over = LongLine::judged_within(&line, None, 1);
    assert!(over.every_value(), "the walk outgrew its budget");
    let first = line.find("v0").expect("a value");
    let shown = over.cut(&line, first - 1..first + 8);
    assert!(!shown.contains("v0"), "{shown}");
    let within = LongLine::judged_within(&line, None, usize::MAX);
    assert!(!within.every_value(), "the walk fitted");
    assert!(
        !LongLine::judged_within("plain text, no structure", None, 1).every_value(),
        "nothing names a walk"
    );
}
