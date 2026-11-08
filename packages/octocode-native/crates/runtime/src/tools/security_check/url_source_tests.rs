//! URLs and purls as coordinates, and the ones that name none.
use super::*;

fn parsed(text: &str) -> (Ecosystem, String, Option<String>) {
    let found = parse(text).unwrap_or_else(|why| panic!("{text}: {why:?}"));
    (found.ecosystem, found.name, found.version)
}

#[test]
fn package_pages_name_their_package_and_version() {
    for (text, ecosystem, name, version) in [
        (
            "https://www.npmjs.com/package/lodash",
            Ecosystem::Npm,
            "lodash",
            None,
        ),
        (
            "https://www.npmjs.com/package/lodash/v/4.17.20",
            Ecosystem::Npm,
            "lodash",
            Some("4.17.20"),
        ),
        (
            "https://www.npmjs.com/package/@babel/traverse",
            Ecosystem::Npm,
            "@babel/traverse",
            None,
        ),
        (
            "https://www.npmjs.com/package/@babel/traverse/v/7.0.0?activeTab=code",
            Ecosystem::Npm,
            "@babel/traverse",
            Some("7.0.0"),
        ),
        (
            "https://pypi.org/project/requests/",
            Ecosystem::PyPi,
            "requests",
            None,
        ),
        (
            "https://pypi.org/project/requests/2.32.0/",
            Ecosystem::PyPi,
            "requests",
            Some("2.32.0"),
        ),
        (
            "https://crates.io/crates/regex",
            Ecosystem::CratesIo,
            "regex",
            None,
        ),
        (
            "https://crates.io/crates/regex/1.5.4",
            Ecosystem::CratesIo,
            "regex",
            Some("1.5.4"),
        ),
        (
            "https://www.nuget.org/packages/Newtonsoft.Json/12.0.3",
            Ecosystem::NuGet,
            "Newtonsoft.Json",
            Some("12.0.3"),
        ),
        (
            "https://rubygems.org/gems/rack",
            Ecosystem::RubyGems,
            "rack",
            None,
        ),
        (
            "https://rubygems.org/gems/rack/versions/2.2.3",
            Ecosystem::RubyGems,
            "rack",
            Some("2.2.3"),
        ),
        (
            "https://packagist.org/packages/symfony/http-kernel",
            Ecosystem::Packagist,
            "symfony/http-kernel",
            None,
        ),
        (
            "https://pkg.go.dev/golang.org/x/text@v0.3.5",
            Ecosystem::Go,
            "golang.org/x/text",
            Some("v0.3.5"),
        ),
        (
            "https://pkg.go.dev/golang.org/x/text/language",
            Ecosystem::Go,
            "golang.org/x/text/language",
            None,
        ),
        (
            "https://mvnrepository.com/artifact/org.apache.logging.log4j/log4j-core/2.14.1",
            Ecosystem::Maven,
            "org.apache.logging.log4j:log4j-core",
            Some("2.14.1"),
        ),
        (
            "https://central.sonatype.com/artifact/org.apache.logging.log4j/log4j-core",
            Ecosystem::Maven,
            "org.apache.logging.log4j:log4j-core",
            None,
        ),
    ] {
        assert_eq!(
            parsed(text),
            (ecosystem, name.to_owned(), version.map(str::to_owned)),
            "{text}"
        );
    }
}

/// A GitHub page is routed to GitHub, never parsed as a package: a registry
/// never hears a repository's name.
#[test]
fn a_github_page_names_a_repository_or_a_commit() {
    let repository = |owner: &str, repo: &str| {
        Some(Ok(GithubUrl::Repository {
            owner: owner.to_owned(),
            repo: repo.to_owned(),
        }))
    };
    assert_eq!(
        github("https://github.com/acme/private.git"),
        repository("acme", "private")
    );
    assert_eq!(
        github("https://www.github.com/acme/tool/tree/main/src"),
        repository("acme", "tool")
    );
    let sha = "4f0b76e2eca13de1c1fe8b4305abc1f7d63f4b86";
    assert_eq!(
        github(&format!(
            "https://github.com/lodash/lodash/commit/{}",
            sha.to_uppercase()
        )),
        Some(Ok(GithubUrl::Commit {
            owner: "lodash".into(),
            repo: "lodash".into(),
            sha: sha.into(),
        }))
    );
    for other in [
        "https://pypi.org/project/requests",
        "https://github.com/acme",
        "https://gitlab.com/acme/tool",
        "acme/tool",
    ] {
        assert!(github(other).is_none(), "{other}");
    }
}

#[test]
fn a_github_advisory_page_names_the_advisory_and_not_a_repository() {
    for text in [
        "https://github.com/advisories/GHSA-jfh8-c2jp-5v3q",
        "https://www.github.com/advisories/ghsa-jfh8-c2jp-5v3q/",
    ] {
        let Some(Ok(GithubUrl::Advisory { id })) = github(text) else {
            panic!("{text}: not an advisory");
        };
        assert!(id.eq_ignore_ascii_case("GHSA-jfh8-c2jp-5v3q"), "{text}");
    }
    for text in [
        "https://github.com/advisories",
        "https://github.com/advisories/CVE-2021-44228",
        "https://github.com/advisories/GHSA-jfh8-c2jp",
    ] {
        let refused = github(text).expect("a github page").expect_err(text);
        assert!(
            refused.reason.contains("github.com/advisories/GHSA-")
                && refused.reason.contains("<owner>/<repo>"),
            "the refusal names what is accepted: {}",
            refused.reason
        );
    }
}

#[test]
fn a_github_path_naming_no_repository_is_refused_naming_what_is_read() {
    for text in [
        "https://github.com/acme/tool%21",
        "https://github.com/acme/to%20ol",
    ] {
        let refused = github(text).expect("a github page").expect_err(text);
        assert!(
            refused.reason.contains("names no GitHub repository")
                && refused.reason.contains("nothing was sent"),
            "{}",
            refused.reason
        );
    }
}

/// One `.git` is the repository's spelling, as `repository::parse` reads it;
/// a second belongs to the name.
#[test]
fn one_dot_git_is_stripped_as_the_repository_parse_does() {
    assert_eq!(
        github("https://github.com/acme/tool.git.git"),
        Some(Ok(GithubUrl::Repository {
            owner: "acme".into(),
            repo: "tool.git".into(),
        }))
    );
}

#[test]
fn a_short_commit_hash_is_refused_before_anything_is_sent() {
    let refused = github("https://github.com/acme/tool/commit/4f0b76e")
        .expect("a github page")
        .expect_err("short hash");
    assert!(
        refused.reason.contains("full 40 or 64"),
        "{}",
        refused.reason
    );
    assert!(
        refused.reason.contains("nothing was sent"),
        "{}",
        refused.reason
    );
}

#[test]
fn purls_name_their_package() {
    for (text, ecosystem, name, version) in [
        (
            "pkg:npm/lodash@4.17.20",
            Ecosystem::Npm,
            "lodash",
            Some("4.17.20"),
        ),
        (
            "pkg:npm/%40babel/traverse@7.0.0",
            Ecosystem::Npm,
            "@babel/traverse",
            Some("7.0.0"),
        ),
        ("pkg:pypi/requests", Ecosystem::PyPi, "requests", None),
        (
            "pkg:cargo/regex@1.5.4?arch=x86",
            Ecosystem::CratesIo,
            "regex",
            Some("1.5.4"),
        ),
        (
            "pkg:maven/org.apache.logging.log4j/log4j-core@2.14.1",
            Ecosystem::Maven,
            "org.apache.logging.log4j:log4j-core",
            Some("2.14.1"),
        ),
        (
            "pkg:golang/golang.org/x/text@v0.3.5",
            Ecosystem::Go,
            "golang.org/x/text",
            Some("v0.3.5"),
        ),
        (
            "pkg:composer/symfony/http-kernel@v4.4.0",
            Ecosystem::Packagist,
            "symfony/http-kernel",
            Some("v4.4.0"),
        ),
        (
            "pkg:gem/rack@2.2.3",
            Ecosystem::RubyGems,
            "rack",
            Some("2.2.3"),
        ),
        (
            "pkg:nuget/Newtonsoft.Json@12.0.3",
            Ecosystem::NuGet,
            "Newtonsoft.Json",
            Some("12.0.3"),
        ),
    ] {
        assert_eq!(
            parsed(text),
            (ecosystem, name.to_owned(), version.map(str::to_owned)),
            "{text}"
        );
    }
}

/// Every purl type of the contract is read; the ecosystem is the contract's.
#[test]
fn purls_of_the_other_ecosystems_name_their_package() {
    for (text, ecosystem, name, version) in [
        (
            "pkg:pub/http@0.12.0",
            Ecosystem::Pub,
            "http",
            Some("0.12.0"),
        ),
        (
            "pkg:hex/phoenix@1.4.0",
            Ecosystem::Hex,
            "phoenix",
            Some("1.4.0"),
        ),
        (
            "pkg:swift/github.com/apple/swift-nio@2.0.0",
            Ecosystem::SwiftUrl,
            "github.com/apple/swift-nio",
            Some("2.0.0"),
        ),
        (
            "pkg:githubactions/tj-actions/changed-files@40.0.0",
            Ecosystem::GitHubActions,
            "tj-actions/changed-files",
            Some("40.0.0"),
        ),
        (
            "pkg:cran/jsonlite@1.8.0",
            Ecosystem::Cran,
            "jsonlite",
            Some("1.8.0"),
        ),
        (
            "pkg:bioconductor/Biostrings@2.70.0",
            Ecosystem::Bioconductor,
            "Biostrings",
            Some("2.70.0"),
        ),
    ] {
        assert_eq!(
            parsed(text),
            (ecosystem, name.to_owned(), version.map(str::to_owned)),
            "{text}"
        );
    }
}

/// A distribution purl names its distribution in the namespace and its release in `distro`.
#[test]
fn distribution_purls_name_the_distribution_and_the_release() {
    for (text, ecosystem, name, release) in [
        (
            "pkg:deb/debian/openssl@1.1.1n-0%2Bdeb11u1?distro=debian-11",
            Ecosystem::Debian,
            "openssl",
            Some("11"),
        ),
        (
            "pkg:apk/alpine/openssl@1.1.1l-r0?arch=x86&distro=alpine-3.14.2",
            Ecosystem::Alpine,
            "openssl",
            Some("v3.14"),
        ),
        (
            "pkg:deb/ubuntu/openssl@3.0.2-0ubuntu1?distro=ubuntu-22.04",
            Ecosystem::Ubuntu,
            "openssl",
            Some("22.04"),
        ),
        (
            "pkg:rpm/rocky/openssl@3.0.7-1?distro=rocky-9",
            Ecosystem::RockyLinux,
            "openssl",
            Some("9"),
        ),
        // A codename names no release this can state; the row says so.
        (
            "pkg:deb/debian/openssl@1.1.1n?distro=bookworm",
            Ecosystem::Debian,
            "openssl",
            None,
        ),
        (
            "pkg:deb/debian/openssl@1.1.1n",
            Ecosystem::Debian,
            "openssl",
            None,
        ),
    ] {
        let found = parse(text).unwrap_or_else(|why| panic!("{text}: {why:?}"));
        assert_eq!(found.ecosystem, ecosystem, "{text}");
        assert_eq!(found.name, name, "{text}");
        assert_eq!(found.release.as_deref(), release, "{text}");
    }
    assert!(
        parse("pkg:deb/debian/openssl@1?distro=debian-11")
            .unwrap()
            .version
            .is_some()
    );
    // A distro qualifier on a package ecosystem is not a release.
    assert_eq!(
        parse("pkg:npm/lodash@4?distro=debian-11").unwrap().release,
        None
    );
}

/// The refusal lists the contract's types, so it cannot go stale.
#[test]
fn an_unknown_purl_type_lists_the_types_the_contract_reads() {
    let why = parse("pkg:unknown/thing@1").expect_err("refused");
    for kind in ecosystem::purl_types().expect("contract") {
        assert!(why.reason.contains(&kind), "{kind}: {}", why.reason);
    }
    // A distribution type whose namespace is no distribution of its list.
    assert!(parse("pkg:deb/notadistro/openssl@1").is_err());
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
    }
    assert!(
        parse("https://example.com/x")
            .expect_err("unknown host")
            .reason
            .contains("cannot be resolved")
    );
}

/// A packagist page names a release by its fragment; one that is no version
/// (a page section) leaves the page as the latest.
#[test]
fn a_packagist_fragment_names_the_version() {
    let at = |fragment: &str| {
        parsed(&format!(
            "https://packagist.org/packages/guzzlehttp/guzzle{fragment}"
        ))
        .2
    };
    assert_eq!(at("#6.3.0").as_deref(), Some("6.3.0"));
    assert_eq!(at("#v7.0.1").as_deref(), Some("7.0.1"));
    assert_eq!(at("#readme"), None);
    assert_eq!(at(""), None);
}
