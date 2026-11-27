use super::alternation;

#[test]
fn each_entry_keeps_its_case_rule_and_literals_are_escaped() {
    assert_eq!(
        alternation([("a.c", false), ("X|y", true)], false),
        r"(?i:a\.c)|(?:X\|y)"
    );
    assert_eq!(
        alternation([("a.c", false), ("X|y", true)], true),
        "(?i:a.c)|(?:X|y)"
    );
}
