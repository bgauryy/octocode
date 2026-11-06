//! URLs and purls as coordinates, and the ones that name none.
use super::*;

fn parsed(text: &str) -> (ArtifactType, String, Option<String>) {
    let found = parse(text).unwrap_or_else(|why| panic!("{text}: {why:?}"));
    (found.kind, found.name, found.version)
}

#[test]
fn package_pages_name_their_package_and_version() {
    for (text, kind, name, version) in [
        (
            "https://www.npmjs.com/package/lodash",
            ArtifactType::Npm,
            "lodash",
            None,
        ),
        (
            "https://www.npmjs.com/package/lodash/v/4.17.20",
            ArtifactType::Npm,
            "lodash",
            Some("4.17.20"),
        ),
        (
            "https://www.npmjs.com/package/@babel/traverse",
            ArtifactType::Npm,
            "@babel/traverse",
            None,
        ),
        (
            "https://www.npmjs.com/package/@babel/traverse/v/7.0.0?activeTab=code",
            ArtifactType::Npm,
            "@babel/traverse",
            Some("7.0.0"),
        ),
        (
            "https://pypi.org/project/requests/",
            ArtifactType::Pypi,
            "requests",
            None,
        ),
        (
            "https://pypi.org/project/requests/2.32.0/",
            ArtifactType::Pypi,
            "requests",
            Some("2.32.0"),
        ),
        (
            "https://crates.io/crates/regex",
            ArtifactType::Crates,
            "regex",
            None,
        ),
        (
            "https://crates.io/crates/regex/1.5.4",
            ArtifactType::Crates,
            "regex",
            Some("1.5.4"),
        ),
        (
            "https://www.nuget.org/packages/Newtonsoft.Json/12.0.3",
            ArtifactType::Nuget,
            "Newtonsoft.Json",
            Some("12.0.3"),
        ),
        (
            "https://rubygems.org/gems/rack",
            ArtifactType::Rubygems,
            "rack",
            None,
        ),
        (
            "https://rubygems.org/gems/rack/versions/2.2.3",
            ArtifactType::Rubygems,
            "rack",
            Some("2.2.3"),
        ),
        (
            "https://packagist.org/packages/symfony/http-kernel",
            ArtifactType::Packagist,
            "symfony/http-kernel",
            None,
        ),
        (
            "https://pkg.go.dev/golang.org/x/text@v0.3.5",
            ArtifactType::Go,
            "golang.org/x/text",
            Some("v0.3.5"),
        ),
        (
            "https://pkg.go.dev/golang.org/x/text/language",
            ArtifactType::Go,
            "golang.org/x/text/language",
            None,
        ),
        (
            "https://mvnrepository.com/artifact/org.apache.logging.log4j/log4j-core/2.14.1",
            ArtifactType::Maven,
            "org.apache.logging.log4j:log4j-core",
            Some("2.14.1"),
        ),
        (
            "https://central.sonatype.com/artifact/org.apache.logging.log4j/log4j-core",
            ArtifactType::Maven,
            "org.apache.logging.log4j:log4j-core",
            None,
        ),
    ] {
        assert_eq!(
            parsed(text),
            (kind, name.to_owned(), version.map(str::to_owned)),
            "{text}"
        );
    }
}

/// Refused before any request: a registry never hears a repository's name.
#[test]
fn a_repository_page_is_refused_with_its_coordinates() {
    let refused = parse("https://github.com/acme/private.git").expect_err("refused");
    assert!(
        refused.reason.contains("is a repository"),
        "{}",
        refused.reason
    );
    assert!(
        refused.reason.contains("nothing was sent"),
        "{}",
        refused.reason
    );
    assert_eq!(
        refused.repository,
        Some(("acme".to_owned(), "private".to_owned()))
    );
    assert!(parse("https://pypi.org/project/requests").is_ok());
}

#[test]
fn purls_name_their_package() {
    for (text, kind, name, version) in [
        (
            "pkg:npm/lodash@4.17.20",
            ArtifactType::Npm,
            "lodash",
            Some("4.17.20"),
        ),
        (
            "pkg:npm/%40babel/traverse@7.0.0",
            ArtifactType::Npm,
            "@babel/traverse",
            Some("7.0.0"),
        ),
        ("pkg:pypi/requests", ArtifactType::Pypi, "requests", None),
        (
            "pkg:cargo/regex@1.5.4?arch=x86",
            ArtifactType::Crates,
            "regex",
            Some("1.5.4"),
        ),
        (
            "pkg:maven/org.apache.logging.log4j/log4j-core@2.14.1",
            ArtifactType::Maven,
            "org.apache.logging.log4j:log4j-core",
            Some("2.14.1"),
        ),
        (
            "pkg:golang/golang.org/x/text@v0.3.5",
            ArtifactType::Go,
            "golang.org/x/text",
            Some("v0.3.5"),
        ),
        (
            "pkg:composer/symfony/http-kernel@v4.4.0",
            ArtifactType::Packagist,
            "symfony/http-kernel",
            Some("v4.4.0"),
        ),
        (
            "pkg:gem/rack@2.2.3",
            ArtifactType::Rubygems,
            "rack",
            Some("2.2.3"),
        ),
        (
            "pkg:nuget/Newtonsoft.Json@12.0.3",
            ArtifactType::Nuget,
            "Newtonsoft.Json",
            Some("12.0.3"),
        ),
    ] {
        assert_eq!(
            parsed(text),
            (kind, name.to_owned(), version.map(str::to_owned)),
            "{text}"
        );
    }
}

#[test]
fn anything_else_says_it_cannot_be_resolved() {
    for text in [
        "https://example.com/package/lodash",
        "https://registry.internal.example/package/lodash",
        "https://www.npmjs.com/settings/me",
        "https://pypi.org/",
        "ftp://pypi.org/project/x",
        "lodash",
        "pkg:unknown/thing@1",
        "pkg:maven/onlyartifact@1",
    ] {
        let why = parse(text).expect_err(text);
        assert!(why.reason.contains(text), "{}", why.reason);
        assert!(why.reason.contains("nothing was sent"), "{}", why.reason);
        assert_eq!(why.repository, None);
    }
    assert!(
        parse("https://example.com/x")
            .expect_err("unknown host")
            .reason
            .contains("cannot be resolved")
    );
}
