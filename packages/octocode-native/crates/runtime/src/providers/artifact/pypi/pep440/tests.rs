//! PEP 440 parsing, ordering and specifiers against `oracle.jsonl`: the
//! answers of Python's `packaging` (the library pip vendors) for the
//! spec's examples, generated once by `oracle.py`.
use super::{Specifiers, Version};
use serde_json::Value;

/// The oracle's rows of one kind: `[kind, inputs..., expected]`.
fn rows(kind: &str) -> Vec<Vec<Value>> {
    let rows = include_str!("oracle.jsonl")
        .lines()
        .map(|line| serde_json::from_str::<Vec<Value>>(line).expect("oracle row"))
        .filter(|row| row[0] == kind)
        .map(|row| row[1..].to_vec())
        .collect::<Vec<_>>();
    assert!(!rows.is_empty(), "the oracle has {kind} rows");
    rows
}

fn text(value: &Value) -> &str {
    value.as_str().expect("oracle string")
}

fn version(value: &Value) -> Version {
    Version::parse(text(value)).unwrap_or_else(|| panic!("{value} is PEP 440"))
}

fn specifiers(value: &Value) -> Specifiers {
    Specifiers::parse(text(value)).unwrap_or_else(|clause| panic!("{value}: {clause}"))
}

/// Alternate spellings normalise to one version, and versions order as
/// the spec's example list: dev < pre < final < local < post, epochs first.
#[test]
fn versions_normalise_and_order_like_packaging() {
    let rows = rows("order");
    assert!(rows.len() > 400, "{} order rows", rows.len());
    for row in rows {
        let (left, right) = (version(&row[0]), version(&row[1]));
        let expected = row[2].as_i64().expect("sign").cmp(&0);
        assert_eq!(left.cmp(&right), expected, "{} vs {}", row[0], row[1]);
        assert_eq!(
            right.cmp(&left),
            expected.reverse(),
            "{} vs {}",
            row[1],
            row[0]
        );
    }
}

#[test]
fn malformed_versions_do_not_parse() {
    for row in rows("version") {
        assert_eq!(
            Version::parse(text(&row[0])).is_some(),
            row[1].as_bool().expect("valid"),
            "{}",
            row[0]
        );
    }
}

/// Each clause's admitted set, with pre-releases allowed: `~=`, `==V.*`,
/// local labels, epochs, and the exclusive `<V`/`>V` rules.
#[test]
fn specifiers_admit_what_packaging_admits() {
    for row in rows("match") {
        let candidate = version(&row[1]);
        assert_eq!(
            specifiers(&row[0]).admits(&candidate, text(&row[1])),
            row[2].as_bool().expect("matched"),
            "{} admits {}",
            row[0],
            row[1]
        );
    }
}

/// The installed choice: the highest match, pre-releases only when a
/// clause names one or no final release matches.
#[test]
fn specifiers_resolve_to_the_release_pip_selects() {
    for row in rows("resolve") {
        let releases = row[1].as_array().expect("releases");
        assert_eq!(
            specifiers(&row[0]).resolve(releases.iter().map(text)),
            row[2].as_str(),
            "{} over {releases:?}",
            row[0]
        );
    }
}

#[test]
fn malformed_specifiers_name_the_clause() {
    for row in rows("specifier") {
        let parsed = Specifiers::parse(text(&row[0]));
        assert_eq!(
            parsed.is_ok(),
            row[1].as_bool().expect("valid"),
            "{}: {parsed:?}",
            row[0]
        );
    }
    assert_eq!(
        Specifiers::parse(">=2.8,<<3").expect_err("<<3"),
        "<<3",
        "the error names the clause"
    );
    // `packaging` accepts an empty set (every release); a lookup with no
    // clause names no version, so it is refused.
    for empty in ["", " , "] {
        assert!(Specifiers::parse(empty).is_err(), "{empty:?}");
    }
}
