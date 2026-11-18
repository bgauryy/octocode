//! The spelling an advisory id is asked by.
use super::canonical_advisory_id;

#[test]
fn the_canonical_spelling_is_the_usual_osv_case() {
    for (given, spelled) in [
        ("ghsa-jfh8-c2jp-5v3q", "GHSA-jfh8-c2jp-5v3q"),
        ("GHSA-JFH8-C2JP-5V3Q", "GHSA-jfh8-c2jp-5v3q"),
        ("cve-2021-44228", "CVE-2021-44228"),
        ("pysec-2020-1", "PYSEC-2020-1"),
        ("go-2020-0015", "GO-2020-0015"),
        ("RUSTSEC-2020-0071", "RUSTSEC-2020-0071"),
    ] {
        assert_eq!(canonical_advisory_id(given), spelled);
    }
}
