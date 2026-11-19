use super::*;

#[test]
fn retained_json_weight_counts_payload_and_structural_overhead() {
    let text = "λ東京😀".repeat(128);
    let string = serde_json::json!(text);
    assert!(heap_bytes(&string) >= text.len());
    let array = serde_json::json!([null, null, string]);
    assert!(heap_bytes(&array) >= 3 * size_of::<serde_json::Value>() + text.len());
    let object = serde_json::json!({"small": 1, "nested": array});
    assert!(heap_bytes(&object) >= 2 * OBJECT_ENTRY_BYTES + heap_bytes(&array));
    let many_fields: serde_json::Value = serde_json::Value::Object(
        (0..128)
            .map(|index| (format!("k{index}"), serde_json::json!(0)))
            .collect(),
    );
    assert!(heap_bytes(&many_fields) > bytes(&many_fields).expect("wire bytes"));
}

#[test]
fn json_size_matches_rendered_bytes_and_utf16() {
    for value in [
        serde_json::json!(null),
        serde_json::json!("quote \" tab \t é 中文 😀"),
        serde_json::json!({"nested": [1, true, "\n"]}),
        serde_json::json!({"message": "héllo \"quoted\"", "items": [1, 2.5, null, {"k": "v"}]}),
    ] {
        let rendered = serde_json::to_string(&value).unwrap();
        assert_eq!(bytes(&value), Some(rendered.len()));
        assert_eq!(utf16_units(&value), Some(rendered.encode_utf16().count()));
    }
    let invalid = std::collections::BTreeMap::from([((1, 2), "value")]);
    assert_eq!(bytes(&invalid), None);
    assert_eq!(utf16_units(&invalid), None);
    assert_eq!(bytes_or(&invalid, usize::MAX), usize::MAX);
    assert_eq!(utf16_units_or(&invalid, 7), 7);
    assert_eq!(bytes_or(&value_of("a"), usize::MAX), 3);
}

#[derive(serde::Serialize)]
enum Shape {
    Unit,
    Newtype(String),
    Tuple(u8, Option<f64>),
    Struct { id: i64, tags: Vec<&'static str> },
}

#[derive(serde::Serialize)]
struct Zoo {
    name: String,
    shapes: Vec<Shape>,
    #[serde(skip_serializing_if = "Option::is_none")]
    skipped: Option<u8>,
    wide: (i128, u128, i8, u16, f32, f64, char, bool, ()),
    bytes: serde_bytes_like::Bytes,
    by_name: std::collections::BTreeMap<String, Vec<u32>>,
    by_number: std::collections::BTreeMap<u32, bool>,
    #[serde(flatten)]
    rest: std::collections::BTreeMap<&'static str, i32>,
}

mod serde_bytes_like {
    pub(super) struct Bytes(pub(super) Vec<u8>);
    impl serde::Serialize for Bytes {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.serialize_bytes(&self.0)
        }
    }
}

/// The size serializer adds up exactly what `serde_json::to_vec` writes,
/// over derived structs and enums of every shape, numbers at their edges,
/// escapes, empty and nested containers, and non-string map keys.
#[test]
fn size_serializer_equals_the_rendered_length() {
    let zoo = Zoo {
        name: "q\" \\ \u{0} \u{1f} \u{7f} \n \r \t \u{8} \u{c} é 中 😀".to_owned(),
        shapes: vec![
            Shape::Unit,
            Shape::Newtype(String::new()),
            Shape::Tuple(255, Some(1.5e300)),
            Shape::Tuple(0, None),
            Shape::Struct {
                id: i64::MIN,
                tags: vec![],
            },
            Shape::Struct {
                id: -1,
                tags: vec!["a", "b\"c"],
            },
        ],
        skipped: None,
        wide: (
            i128::MIN,
            u128::MAX,
            -128,
            65535,
            f32::NAN,
            f64::INFINITY,
            '\u{1}',
            false,
            (),
        ),
        bytes: serde_bytes_like::Bytes(vec![0, 9, 10, 99, 100, 255]),
        by_name: [
            ("k\"".to_owned(), vec![1, 10, 100]),
            (String::new(), vec![]),
        ]
        .into(),
        by_number: [(0, true), (u32::MAX, false)].into(),
        rest: [("x", -10), ("y", 0)].into(),
    };
    assert_eq!(bytes(&zoo), Some(serde_json::to_vec(&zoo).unwrap().len()));
    let with = Zoo {
        skipped: Some(7),
        ..zoo_again()
    };
    assert_eq!(bytes(&with), Some(serde_json::to_vec(&with).unwrap().len()));
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..2000 {
        let value = random_value(&mut next, 4);
        assert_eq!(
            bytes(&value),
            Some(serde_json::to_vec(&value).unwrap().len()),
            "{value}"
        );
    }
}

fn zoo_again() -> Zoo {
    Zoo {
        name: String::new(),
        shapes: vec![],
        skipped: None,
        wide: (0, 0, 0, 0, 0.0, 0.0, 'a', true, ()),
        bytes: serde_bytes_like::Bytes(vec![]),
        by_name: Default::default(),
        by_number: Default::default(),
        rest: Default::default(),
    }
}

fn random_value(next: &mut impl FnMut() -> u64, depth: u32) -> serde_json::Value {
    use serde_json::Value;
    match next() % if depth == 0 { 6 } else { 8 } {
        0 => Value::Null,
        1 => Value::Bool(next().is_multiple_of(2)),
        2 => Value::from(next() as i64 >> (next() % 64)),
        3 => Value::from(next() >> (next() % 64)),
        4 => Value::from((next() as f64) / 7.0 - 1e9),
        5 => Value::String(
            (0..next() % 12)
                .map(|_| {
                    ['a', '"', '\\', '\n', '\u{2}', 'é', '😀', '中', ' '][(next() % 9) as usize]
                })
                .collect(),
        ),
        6 => Value::Array(
            (0..next() % 4)
                .map(|_| random_value(next, depth - 1))
                .collect(),
        ),
        _ => Value::Object(
            (0..next() % 4)
                .map(|index| {
                    (
                        format!("k{index}\"{}", next() % 3),
                        random_value(next, depth - 1),
                    )
                })
                .collect(),
        ),
    }
}

fn value_of(text: &str) -> serde_json::Value {
    serde_json::json!(text)
}

#[test]
fn a_string_body_counts_escapes_and_utf16_units_as_rendered() {
    assert_eq!(string_body_units("a\"b\u{1F600}"), 6);
    assert_eq!(string_body_units(""), 0);
    assert_eq!(string_body_units("tab\t"), 5);
}

/// Scalars at every edge the sizer weighs apart: every ASCII byte (each
/// control character `serde_json` escapes, `"`, `\`, `/` and DEL, which it
/// does not), the UTF-8 length boundaries, the code points around the
/// surrogate range, the line and paragraph separators, BMP and astral text.
fn edge_scalars() -> Vec<char> {
    let mut scalars: Vec<char> = (0u8..0x80).map(char::from).collect();
    scalars.extend([
        '\u{80}',
        '\u{7ff}',
        '\u{800}',
        '\u{d7ff}',
        '\u{e000}',
        '\u{feff}',
        '\u{fffd}',
        '\u{ffff}',
        '\u{10000}',
        '\u{10ffff}',
        '\u{2028}',
        '\u{2029}',
        'é',
        '中',
        '😀',
    ]);
    scalars
}

fn edge_string() -> impl proptest::strategy::Strategy<Value = String> {
    use proptest::prelude::*;
    let scalars = edge_scalars();
    let short = proptest::collection::vec(proptest::sample::select(scalars.clone()), 0..40)
        .prop_map(|chars| chars.into_iter().collect::<String>());
    // Long strings cross the sizer's eight-byte steps at every alignment.
    let long = (
        proptest::collection::vec(proptest::sample::select(scalars), 1..12),
        1usize..400,
    )
        .prop_map(|(chars, times)| chars.into_iter().collect::<String>().repeat(times));
    prop_oneof![4 => short, 1 => long]
}

fn edge_value() -> impl proptest::strategy::Strategy<Value = serde_json::Value> {
    use proptest::prelude::*;
    use serde_json::Value;
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(Value::from),
        any::<u64>().prop_map(Value::from),
        any::<f64>().prop_map(Value::from),
        edge_string().prop_map(Value::String),
    ];
    leaf.prop_recursive(4, 64, 6, |inner| {
        prop_oneof![
            proptest::collection::vec(inner.clone(), 0..6).prop_map(Value::Array),
            proptest::collection::vec((edge_string(), inner), 0..6)
                .prop_map(|fields| Value::Object(fields.into_iter().collect())),
        ]
    })
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(512))]

    /// The direct sizer is exact: on every generated value it equals the
    /// writer-based count (`serde_json` rendering into a counter, the sizer
    /// `utf16_units` used before), and both equal the rendered text, in UTF-16
    /// units and in bytes.
    #[test]
    fn the_direct_sizer_equals_the_rendered_count(value in edge_value()) {
        let rendered = serde_json::to_string(&value).unwrap();
        proptest::prop_assert_eq!(utf16_units(&value), count(&value, true));
        proptest::prop_assert_eq!(utf16_units(&value), Some(rendered.encode_utf16().count()));
        proptest::prop_assert_eq!(bytes(&value), count(&value, false));
        proptest::prop_assert_eq!(bytes(&value), Some(rendered.len()));
    }

    /// A string body and raw UTF-8 text are weighed alike: escaped and
    /// unquoted, or as their UTF-16 length.
    #[test]
    fn string_bodies_and_raw_text_count_as_rendered(text in edge_string()) {
        let rendered = serde_json::to_string(&text).unwrap();
        proptest::prop_assert_eq!(string_body_units(&text), rendered.encode_utf16().count() - 2);
        proptest::prop_assert_eq!(utf16_units_of_utf8(text.as_bytes()), text.encode_utf16().count());
    }
}

/// Every single scalar, alone and at each offset of an eight-byte step,
/// sizes as it renders.
#[test]
fn every_edge_scalar_at_every_alignment_sizes_as_rendered() {
    for scalar in edge_scalars() {
        for pad in 0..16 {
            let text = format!("{}{scalar}{}", "a".repeat(pad), "b".repeat(15 - pad));
            let value = serde_json::json!({ text.clone(): [text.clone()] });
            let rendered = serde_json::to_string(&value).unwrap();
            assert_eq!(
                utf16_units(&value),
                Some(rendered.encode_utf16().count()),
                "{scalar:?} at {pad}"
            );
            assert_eq!(bytes(&value), Some(rendered.len()), "{scalar:?} at {pad}");
            assert_eq!(
                utf16_units_of_utf8(text.as_bytes()),
                text.encode_utf16().count()
            );
        }
    }
}
