use super::runtime_dependencies;

/// log4j-core 2.14.1 lists `junit-vintage-engine` with no `<scope>`; its
/// parent manages it as `test`, so it is no runtime dependency. A managed
/// `<optional>` carries over the same way, and a stated scope wins.
#[test]
fn an_unscoped_dependency_takes_its_managed_scope_and_optional() {
    let child = "<project><dependencies>\
        <dependency><groupId>org.junit.vintage</groupId><artifactId>junit-vintage-engine</artifactId></dependency>\
        <dependency><groupId>com.lmax</groupId><artifactId>disruptor</artifactId></dependency>\
        <dependency><groupId>org.slf4j</groupId><artifactId>slf4j-api</artifactId><scope>runtime</scope></dependency>\
        <dependency><groupId>org.osgi</groupId><artifactId>core</artifactId></dependency>\
        </dependencies></project>";
    let parent = "<project><dependencyManagement><dependencies>\
        <dependency><groupId>org.junit.vintage</groupId><artifactId>junit-vintage-engine</artifactId><version>5.7.0</version><scope>test</scope></dependency>\
        <dependency><groupId>com.lmax</groupId><artifactId>disruptor</artifactId><version>3.4.2</version><optional>true</optional></dependency>\
        <dependency><groupId>org.slf4j</groupId><artifactId>slf4j-api</artifactId><scope>test</scope></dependency>\
        </dependencies></dependencyManagement></project>";
    let found = runtime_dependencies(child, &[parent.to_owned()], &[]);
    assert_eq!(found.required, ["org.slf4j:slf4j-api", "org.osgi:core"]);
    assert_eq!(found.optional, ["com.lmax:disruptor"]);
    assert!(found.unresolved.is_empty(), "{:?}", found.unresolved);
}

/// A dependency that states no scope takes the parent's managed `test`
/// scope (mockito-core) and is no runtime dependency; an unmanaged-scope
/// sibling stays compile.
#[test]
fn a_parent_managed_test_scope_keeps_mockito_out_of_the_runtime_dependencies() {
    let child = include_str!("../fixtures/mockito-user.pom");
    let parent = include_str!("../fixtures/mockito-parent.pom");
    let found = runtime_dependencies(child, &[parent.to_owned()], &[]);
    assert_eq!(found.required, ["org.slf4j:slf4j-api"]);
    assert!(found.optional.is_empty());
    assert_eq!(found.unmanaged, 0);
}

/// netty-handler names `${project.groupId}:netty-common` and a
/// parent's `${tcnative.artifactId}`: both resolve to coordinates.
#[test]
fn placeholders_in_group_and_artifact_resolve_from_project_and_parents() {
    let child = "<project><parent><groupId>io.netty</groupId><artifactId>netty-parent</artifactId><version>1</version></parent>\
        <artifactId>netty-handler</artifactId><dependencies>\
        <dependency><groupId>${project.groupId}</groupId><artifactId>netty-common</artifactId><version>${project.version}</version></dependency>\
        <dependency><groupId>${tcnative.groupId}</groupId><artifactId>${tcnative.artifactId}</artifactId><optional>true</optional></dependency>\
        </dependencies></project>";
    let parent = "<project><properties><tcnative.groupId>io.netty</tcnative.groupId><tcnative.artifactId>netty-tcnative</tcnative.artifactId></properties></project>";
    let found = runtime_dependencies(child, &[parent.to_owned()], &[]);
    assert_eq!(found.required, ["io.netty:netty-common:1"]);
    assert_eq!(found.optional, ["io.netty:netty-tcnative"]);
    assert!(found.unresolved.is_empty(), "{:?}", found.unresolved);
}

/// A property no read POM defines is stated by name; the dependency is
/// not listed as a coordinate that contains it.
#[test]
fn an_undefined_placeholder_is_disclosed_and_never_listed_as_a_coordinate() {
    let child = "<project><dependencies>\
        <dependency><groupId>x</groupId><artifactId>${missing.artifactId}</artifactId></dependency>\
        <dependency><groupId>x</groupId><artifactId>ok</artifactId></dependency>\
        <dependency><groupId>x</groupId><artifactId>${test.only}</artifactId><scope>test</scope></dependency>\
        </dependencies></project>";
    let found = runtime_dependencies(child, &[], &[]);
    assert_eq!(found.required, ["x:ok"]);
    assert_eq!(found.unresolved, ["${missing.artifactId}"]);
    // Only `x:ok` awaits a management entry: the dropped one needs no BOM read.
    assert_eq!(found.unmanaged, 1);
}

/// Maven manages by groupId:artifactId:type:classifier (jar, none): a
/// managed `test-jar` does not scope the plain jar.
#[test]
fn a_managed_test_jar_does_not_scope_the_plain_jar() {
    let child = "<project><dependencies>\
        <dependency><groupId>x</groupId><artifactId>a</artifactId></dependency>\
        <dependency><groupId>x</groupId><artifactId>a</artifactId><type>test-jar</type></dependency>\
        <dependency><groupId>x</groupId><artifactId>b</artifactId><classifier>tests</classifier></dependency>\
        </dependencies></project>";
    let parent = "<project><dependencyManagement><dependencies>\
        <dependency><groupId>x</groupId><artifactId>a</artifactId><type>test-jar</type><scope>test</scope></dependency>\
        <dependency><groupId>x</groupId><artifactId>b</artifactId><scope>test</scope></dependency>\
        </dependencies></dependencyManagement></project>";
    let found = runtime_dependencies(child, &[parent.to_owned()], &[]);
    assert_eq!(found.required, ["x:a", "x:b"]);
}
