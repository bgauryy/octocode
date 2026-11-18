use super::*;

fn pin(kind: ArtifactType, entry: &str) -> Option<(String, String)> {
    pin_of(kind, entry).map(|pin| (pin.name, pin.version))
}

fn of(kind: ArtifactType, entry: &str, name: &str, version: &str) {
    assert_eq!(
        pin(kind, entry),
        Some((name.to_owned(), version.to_owned())),
        "{entry}"
    );
}

fn none(kind: ArtifactType, entry: &str) {
    assert_eq!(pin(kind, entry), None, "{entry}");
}

#[test]
fn each_registry_reads_only_a_single_version_requirement_as_a_pin() {
    of(
        ArtifactType::Rubygems,
        "actionpack = 8.1.4.1",
        "actionpack",
        "8.1.4.1",
    );
    none(ArtifactType::Rubygems, "bundler >= 1.15.0");
    none(ArtifactType::Rubygems, "nokogiri ~> 1.8");
    of(ArtifactType::Npm, "lodash@4.17.21", "lodash", "4.17.21");
    of(
        ArtifactType::Npm,
        "@babel/core@=7.0.0-rc.1",
        "@babel/core",
        "7.0.0-rc.1",
    );
    none(ArtifactType::Npm, "lodash@^4.17.0");
    none(ArtifactType::Npm, "lodash@4.x");
    none(ArtifactType::Npm, "lodash@>=4 <5");
    none(ArtifactType::Npm, "lodash@latest");
    of(ArtifactType::Pypi, "idna==3.7", "idna", "3.7");
    of(
        ArtifactType::Pypi,
        "urllib3[socks] (==1.26.5) ; python_version >= '3'",
        "urllib3",
        "1.26.5",
    );
    none(ArtifactType::Pypi, "urllib3<3,>=1.21.1");
    none(ArtifactType::Pypi, "idna==3.*");
    of(
        ArtifactType::Crates,
        "serde_derive =1.0.100",
        "serde_derive",
        "1.0.100",
    );
    of(
        ArtifactType::Crates,
        "winapi =0.3.9; cfg(windows)",
        "winapi",
        "0.3.9",
    );
    none(ArtifactType::Crates, "serde ^1.0");
    none(ArtifactType::Crates, "serde >=1, <2");
    of(
        ArtifactType::Maven,
        "org.apache.logging.log4j:log4j-api:2.14.1",
        "org.apache.logging.log4j:log4j-api",
        "2.14.1",
    );
    none(ArtifactType::Maven, "org.apache.logging.log4j:log4j-api");
    none(ArtifactType::Maven, "com.x:y:${y.version}");
    none(ArtifactType::Maven, "com.x:y:[1.0,2.0)");
    of(
        ArtifactType::Nuget,
        "Newtonsoft.Json [9.0.1]",
        "Newtonsoft.Json",
        "9.0.1",
    );
    of(
        ArtifactType::Nuget,
        "Pkg [1.0.0, 1.0.0] (net6.0)",
        "Pkg",
        "1.0.0",
    );
    none(ArtifactType::Nuget, "Newtonsoft.Json 9.0.1");
    none(ArtifactType::Nuget, "Pkg [1.0.0, )");
    of(
        ArtifactType::Packagist,
        "symfony/http-kernel =6.3.0",
        "symfony/http-kernel",
        "6.3.0",
    );
    none(ArtifactType::Packagist, "php ^8.1");
    none(ArtifactType::Packagist, "php 8.1.0");
    none(ArtifactType::Packagist, "psr/log ^1.0 || ^2.0");
    none(ArtifactType::Go, "golang.org/x/text v0.3.0");
}
