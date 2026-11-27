//! The CVSS rating a vector-only advisory ranks by: the specifications' own
//! worked examples, each base score and its rating.
use super::*;
use serde_json::json;

/// FIRST's CVSS v3.1 examples document (and CVE-2023-25193), with the base
/// score each publishes.
#[test]
fn v3_vectors_score_as_the_specification_examples_do() {
    for (vector, score, rated) in [
        // CVE-2013-1937, phpMyAdmin reflected XSS.
        (
            "CVSS:3.1/AV:N/AC:L/PR:N/UI:R/S:C/C:L/I:L/A:N",
            6.1,
            "medium",
        ),
        // CVE-2014-3566, SSLv3 POODLE.
        ("CVSS:3.1/AV:N/AC:H/PR:N/UI:R/S:U/C:L/I:N/A:N", 3.1, "low"),
        // CVE-2012-1516, VMware guest to host escape.
        (
            "CVSS:3.1/AV:N/AC:L/PR:L/UI:N/S:C/C:H/I:H/A:H",
            9.9,
            "critical",
        ),
        // CVE-2009-0783, Apache Tomcat XML parser.
        (
            "CVSS:3.1/AV:L/AC:L/PR:H/UI:N/S:U/C:L/I:L/A:L",
            4.2,
            "medium",
        ),
        // CVE-2014-0160, Heartbleed.
        ("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:N/A:N", 7.5, "high"),
        // CVE-2014-6271, Shellshock.
        (
            "CVSS:3.0/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H",
            9.8,
            "critical",
        ),
        // CVE-2012-0384, Cisco IOS command authorization bypass.
        ("CVSS:3.1/AV:N/AC:L/PR:L/UI:N/S:U/C:H/I:H/A:H", 8.8, "high"),
        // CVE-2023-25193, HarfBuzz: the vector OSV gives with no word.
        ("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:H", 7.5, "high"),
        // No impact: the scale's None.
        ("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:N", 0.0, "none"),
    ] {
        let computed = v3_base(vector).expect(vector);
        assert!((computed - score).abs() < 1e-9, "{vector}: {computed}");
        assert_eq!(
            rating(&[json!({"type": "CVSS_V3", "vector": vector})]),
            Some(rated),
            "{vector}"
        );
    }
}

/// The CVSS v2 guide's examples, rated on NVD's v2 scale.
#[test]
fn v2_vectors_score_as_the_guide_examples_do() {
    for (vector, score, rated) in [
        // CVE-2002-0392, Apache chunked encoding.
        ("AV:N/AC:L/Au:N/C:N/I:N/A:C", 7.8, "high"),
        // CVE-2003-0818, Microsoft ASN.1.
        ("AV:N/AC:L/Au:N/C:C/I:C/A:C", 10.0, "high"),
        // CVE-2003-0062, NOD32 buffer overflow.
        ("AV:L/AC:H/Au:N/C:C/I:C/A:C", 6.2, "medium"),
    ] {
        let computed = v2_base(vector).expect(vector);
        assert!((computed - score).abs() < 1e-9, "{vector}: {computed}");
        for spelled in [vector.to_owned(), format!("({vector})")] {
            assert_eq!(
                rating(&[json!({"type": "CVSS_V2", "vector": spelled})]),
                Some(rated),
                "{spelled}"
            );
        }
    }
}

/// A published score rates as published; v3.x outranks v2; a v4 vector
/// alone, or one missing a base metric, rates nothing.
#[test]
fn a_published_score_rates_first_and_an_unreadable_vector_rates_nothing() {
    let published = json!({
        "type": "CVSS_V3",
        "vector": "CVSS:3.1/AV:N/AC:H/PR:N/UI:R/S:U/C:L/I:N/A:N",
        "score": 9.1
    });
    assert_eq!(rating(&[published]), Some("critical"));
    let both = [
        json!({"type": "CVSS_V2", "vector": "AV:N/AC:L/Au:N/C:C/I:C/A:C"}),
        json!({"type": "CVSS_V3", "vector": "CVSS:3.1/AV:N/AC:H/PR:N/UI:R/S:U/C:L/I:N/A:N"}),
    ];
    assert_eq!(rating(&both), Some("low"));
    let v4 = json!({
        "type": "CVSS_V4",
        "vector": "CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N/VC:H/VI:H/VA:H/SC:N/SI:N/SA:N"
    });
    assert_eq!(rating(&[v4]), None);
    let partial = json!({"type": "CVSS_V3", "vector": "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H"});
    assert_eq!(rating(&[partial]), None);
    assert_eq!(rating(&[]), None);
}
