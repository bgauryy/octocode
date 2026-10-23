//! Compile-once XML and coordinate patterns for Maven metadata and POMs.
use regex::Regex;
use std::sync::LazyLock;

macro_rules! pattern {
    ($name:ident, $src:expr) => {
        pub(super) fn $name() -> &'static Regex {
            static RE: LazyLock<Regex> = LazyLock::new(|| {
                Regex::new($src).expect(concat!("static maven pattern: ", stringify!($name)))
            });
            &RE
        }
    };
}

pattern!(valid_coordinate, r"^[A-Za-z0-9_][A-Za-z0-9_.-]*$");
pattern!(doctype, r"(?i)<!DOCTYPE|<!ENTITY");
pattern!(comment, r"(?s)<!--.*?-->");
pattern!(url_tag, r"<url>\s*([^<]+?)\s*</url>");
pattern!(scm, r"(?s)<scm>(.*?)</scm>");
pattern!(group_id, r"<groupId>\s*([^<]+?)\s*</groupId>");
pattern!(artifact_id, r"<artifactId>\s*([^<]+?)\s*</artifactId>");
pattern!(release, r"<release>\s*([^<]+?)\s*</release>");
pattern!(latest, r"<latest>\s*([^<]+?)\s*</latest>");
pattern!(version, r"<version>\s*([^<]+?)\s*</version>");
pattern!(
    inherited_blocks,
    r"(?s)<(dependencyManagement|build|profiles|reporting)>.*?</(dependencyManagement|build|profiles|reporting)>"
);
pattern!(dependencies, r"(?s)<dependencies>(.*?)</dependencies>");
pattern!(dependency, r"(?s)<dependency>(.*?)</dependency>");
pattern!(exclusions, r"(?s)<exclusions>.*?</exclusions>");
pattern!(scope, r"<scope>\s*([^<]+?)\s*</scope>");
pattern!(optional, r"<optional>\s*([^<]+?)\s*</optional>");
pattern!(parent, r"(?s)<parent>(.*?)</parent>");
pattern!(properties, r"(?s)<properties>(.*?)</properties>");
pattern!(
    property,
    r"<([A-Za-z0-9_.\-]+)>\s*([^<]*?)\s*</([A-Za-z0-9_.\-]+)>"
);
pattern!(reference, r"\$\{([^}]+)\}");

/// The bounded set of `<tag>value</tag>` fields extracted from
/// `maven-metadata.xml`. Unknown tags return `None` (the `regex` crate has
/// no backreferences, so a single generic `<(\w+)>…</\1>` is unavailable).
pub(super) fn field(tag: &str) -> Option<&'static Regex> {
    Some(match tag {
        "groupId" => group_id(),
        "artifactId" => artifact_id(),
        "release" => release(),
        "latest" => latest(),
        _ => return None,
    })
}
