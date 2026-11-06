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
pattern!(connection, r"<connection>\s*([^<]+?)\s*</connection>");
pattern!(
    developer_connection,
    r"<developerConnection>\s*([^<]+?)\s*</developerConnection>"
);
pattern!(scm_tag, r"<tag>\s*([^<]+?)\s*</tag>");
// One opening, closing or self-closing element tag (not a declaration,
// processing instruction or comment), for walking a POM's nesting.
pattern!(
    element,
    r"<(/?)([A-Za-z_][A-Za-z0-9_.-]*)(?:\s[^>]*?)?(/?)>"
);
pattern!(group_id, r"<groupId>\s*([^<]+?)\s*</groupId>");
pattern!(artifact_id, r"<artifactId>\s*([^<]+?)\s*</artifactId>");
pattern!(release, r"<release>\s*([^<]+?)\s*</release>");
pattern!(latest, r"<latest>\s*([^<]+?)\s*</latest>");
pattern!(version, r"<version>\s*([^<]+?)\s*</version>");
pattern!(dependency, r"(?s)<dependency>(.*?)</dependency>");
pattern!(exclusions, r"(?s)<exclusions>.*?</exclusions>");
pattern!(scope, r"<scope>\s*([^<]+?)\s*</scope>");
pattern!(optional, r"<optional>\s*([^<]+?)\s*</optional>");
pattern!(
    property,
    r"<([A-Za-z0-9_.\-]+)>\s*([^<]*?)\s*</([A-Za-z0-9_.\-]+)>"
);
pattern!(reference, r"\$\{([^}]+)\}");
// `ComparableVersion`'s prerelease qualifiers and their one-letter aliases
// (which need a digit right after them, so `-b` alone is not a beta), plus the
// pre-final words it does not know, which it would order after a final release.
pattern!(
    prerelease,
    r"(?i)(?:^|[^a-z])(?:(?:alpha|beta|milestone|rc|cr|snapshot|pre|preview|dev|ea|eap|nightly)(?:[^a-z]|$)|[abm][0-9])"
);

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
