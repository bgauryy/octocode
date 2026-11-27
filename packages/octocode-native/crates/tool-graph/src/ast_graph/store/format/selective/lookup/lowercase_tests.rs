use super::lowercase_order;

/// The order the index was built in: `to_lowercase` of each name.
#[test]
fn names_compare_as_their_lowercase_copies_do() {
    for (a, b) in [
        ("Alpha", "alpha"),
        ("alpha", "Beta"),
        ("a_b", "aB"),
        ("Z", "a"),
        ("", "a"),
        ("Äpfel", "äpfel"),
        ("İx", "ix"),
        ("ß", "SS"),
        ("Σ", "σ"),
    ] {
        assert_eq!(
            lowercase_order(a, b),
            a.to_lowercase().cmp(&b.to_lowercase()),
            "{a} {b}"
        );
    }
}
