//! The compact form against JSON: the same data back, and the same JSON size.
use super::super::compact::{pack, put_varint, take_varint, unpack};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// `value` packed, unpacked equal, and its size counted as JSON's own.
fn round_trip<T>(value: &T)
where
    T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
{
    let packed = pack(value, usize::MAX >> 8).expect("packs");
    assert_eq!(
        packed.json_bytes,
        serde_json::to_vec(value).unwrap().len(),
        "json size of {value:?}"
    );
    let back: T = unpack(&packed.bytes).expect("unpacks");
    assert_eq!(&back, value);
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
enum Shape {
    Unit,
    Newtype(u32),
    Tuple(i64, String),
    Struct { name: String, line: u32 },
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum Tagged {
    Parsed { range: (u32, u32) },
    Partial { reason: String },
    Empty,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
#[serde(tag = "kind", content = "at", rename_all = "camelCase")]
enum Adjacent {
    Root(u32),
    Span { start: u32, end: u32 },
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
struct Record {
    long_field_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    maybe: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    items: Vec<u32>,
    flag: bool,
    shapes: Vec<Shape>,
    tagged: Vec<Tagged>,
    adjacent: Vec<Adjacent>,
    by_name: BTreeMap<String, Vec<u8>>,
    wide: (u64, i64, i8, u16),
    ratio: f64,
}

fn record(seed: usize) -> Record {
    Record {
        long_field_name: format!("declaration:src/lib.rs#name_{seed}@12:function"),
        maybe: seed.is_multiple_of(2).then(|| "x".repeat(seed % 300)),
        items: (0..seed as u32).map(|n| n * 97).collect(),
        flag: seed.is_multiple_of(3),
        shapes: vec![
            Shape::Unit,
            Shape::Newtype(u32::MAX),
            Shape::Tuple(i64::MIN, "tab\there \"quoted\" \\ \u{1}\u{1f} é 😀".into()),
            Shape::Struct {
                name: "declaration:src/lib.rs#name@12:function".into(),
                line: 7,
            },
        ],
        tagged: vec![
            Tagged::Parsed { range: (1, 128) },
            Tagged::Partial {
                reason: "unresolved".into(),
            },
            Tagged::Empty,
        ],
        adjacent: vec![Adjacent::Root(0), Adjacent::Span { start: 5, end: 9 }],
        by_name: [
            ("alpha".to_owned(), vec![1, 2, 200]),
            ("beta".to_owned(), vec![]),
        ]
        .into(),
        wide: (u64::MAX, i64::MIN, -128, 65535),
        ratio: [0.0, 1.5, -2.25e-9, 1e300][seed % 4],
    }
}

#[test]
fn the_derived_shapes_round_trip_with_their_json_size() {
    for seed in [0, 1, 2, 3, 4, 5, 127, 128, 129, 300] {
        round_trip(&record(seed));
    }
    round_trip(&Vec::<Record>::new());
    round_trip(&(1_u8, "a".to_owned(), None::<u32>, Some(3_u32)));
}

#[test]
fn json_values_round_trip_with_their_json_size() {
    let long = "q".repeat(5000);
    for text in [
        r#"null"#,
        r#"[true,false,0,127,128,16383,16384,18446744073709551615,-1,-9223372036854775808]"#,
        r#"{"a":[],"b":{},"c":"","d":"ab","e":"abcd","f":"abcd","g":["abcd","abcdef","abcd"]}"#,
        r#"{"é":"\u0000\u001f\n\r\t\b\f\"\\ 😀","k":1.5,"l":-0.0,"m":1e300,"n":[[[[[]]]]]}"#,
    ] {
        let value: serde_json::Value = serde_json::from_str(text).unwrap();
        round_trip(&value);
    }
    round_trip(&serde_json::json!({"long": long, "again": long}));
}

#[test]
fn the_index_survives_growth_and_every_string_comes_back_where_it_was() {
    // More distinct strings than the index's first table holds, each used
    // twice, in an order that makes the second use a reference.
    let words: Vec<String> = (0..5000).map(|n| format!("word-{n}")).collect();
    let mut value = words.clone();
    value.extend(words.iter().rev().cloned());
    round_trip(&value);
    let packed = pack(&value, usize::MAX >> 8).unwrap();
    let plain = serde_json::to_vec(&value).unwrap();
    assert!(
        packed.bytes.len() < plain.len() * 3 / 4,
        "{} against {}",
        packed.bytes.len(),
        plain.len()
    );
}

#[test]
fn non_finite_floats_count_as_the_null_json_writes() {
    let packed = pack(&[f64::NAN, f64::INFINITY], 1 << 20).unwrap();
    assert_eq!(packed.json_bytes, "[null,null]".len());
}

#[test]
fn a_record_past_the_bound_is_refused_and_one_at_it_is_not() {
    let value = record(40);
    let size = serde_json::to_vec(&value).unwrap().len();
    assert!(pack(&value, size).is_ok());
    assert!(pack(&value, size - 1).is_err());
}

#[test]
fn damaged_records_fail_without_panicking() {
    let packed = pack(&record(9), 1 << 20).unwrap().bytes;
    // Every proper prefix is a record that ends inside a value.
    for cut in 0..packed.len() {
        assert!(unpack::<Record>(&packed[..cut]).is_err(), "cut at {cut}");
    }
    let mut tail = packed.clone();
    tail.push(0);
    assert!(unpack::<Record>(&tail).is_err(), "trailing byte");
    // One byte changed anywhere never panics; it fails or reads as other data.
    for at in 0..packed.len() {
        for byte in [0x00, 0x07, 0x0b, 0x0c, 0x7f, 0xff] {
            let mut damaged = packed.clone();
            damaged[at] = byte;
            let _ = unpack::<Record>(&damaged);
        }
    }
    assert!(unpack::<Record>(&[7, 3]).is_err(), "reference before write");
    assert!(unpack::<Record>(&[0x0c]).is_err(), "unknown tag");
    let nested = [9_u8; 300];
    assert!(unpack::<serde_json::Value>(&nested).is_err(), "depth");
}

#[test]
fn a_number_keyed_map_is_no_record() {
    let keyed: BTreeMap<u32, u32> = [(1, 2)].into();
    assert!(pack(&keyed, 1 << 10).is_err());
}

#[test]
fn varints_round_trip() {
    for n in [0, 1, 127, 128, 16383, 16384, u32::MAX as u64, u64::MAX] {
        let mut out = Vec::new();
        put_varint(&mut out, n);
        out.push(0xAA);
        assert_eq!(take_varint(&out), Some((n, &[0xAA][..])), "{n}");
    }
    assert_eq!(take_varint(&[0x80; 11]), None);
}

/// Real extractions: every declaration, call, argument and edge the facts of
/// the languages the graph reads hold, as an artifact writes them.
#[test]
fn extracted_facts_round_trip_as_an_artifact_writes_them() {
    let sources: [(&str, &str); 6] = [
        ("src/a.rs", include_str!("../compact.rs")),
        ("src/b.rs", include_str!("../../scan.rs")),
        (
            "src/c.ts",
            "import {a as b} from './x';\nexport class K<T> extends Base { m(x: T) { return b(x).y?.z(1, 'two'); } }\nconst s = `t${1}`;\n",
        ),
        (
            "src/d.py",
            "import os\nclass A(B):\n    def f(self, x):\n        return os.path.join(x, 'é')\n",
        ),
        (
            "src/e.go",
            "package p\nimport \"fmt\"\nfunc F(x int) { fmt.Println(x) }\n",
        ),
        (
            "src/F.java",
            "package p;\nclass F { void g() { Util.h(1, \"s\"); } }\n",
        ),
    ];
    for (file, source) in sources {
        let facts = octocode_engine::signatures::extract_graph_facts(source, file)
            .unwrap_or_else(|| panic!("{file} parses"));
        let counts = facts
            .declarations
            .iter()
            .enumerate()
            .map(|(n, declaration)| (declaration.id.as_str(), n as u32))
            .collect::<Vec<_>>();
        let artifact = super::super::cache::Artifact {
            namespace: "ns",
            relative_path: file,
            content_digest: "digest",
            facts: &facts,
            reference_counts: counts,
        };
        let packed = pack(&artifact, usize::MAX >> 8).expect("packs");
        let json = serde_json::to_vec(&artifact).unwrap();
        assert_eq!(packed.json_bytes, json.len(), "{file}: json size");
        let back: super::super::cache::OwnedArtifact = unpack(&packed.bytes).expect("unpacks");
        assert_eq!(back.facts, facts, "{file}");
        assert_eq!(
            serde_json::to_vec(&back.facts).unwrap(),
            serde_json::to_vec(&facts).unwrap(),
            "{file}: the facts' JSON"
        );
        assert!(
            packed.bytes.len() * 4 < json.len() * 3,
            "{file}: {} compact against {} json",
            packed.bytes.len(),
            json.len()
        );
    }
}
