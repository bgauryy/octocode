use super::*;

#[test]
fn registry_capabilities_are_part_of_the_artifact_namespace() {
    let root = Path::new("/workspace");
    let scan = GraphScan::default();
    let installed = namespace(&scan, root, RegistryView::installed()).unwrap();
    let injected = namespace(
        &scan,
        root,
        octocode_engine::signatures::languages::conformance::toml_registry(),
    )
    .unwrap();
    assert_ne!(installed, injected);
    assert_eq!(
        installed,
        namespace(&scan, root, RegistryView::installed()).unwrap()
    );
}

#[test]
fn the_known_format_list_is_part_of_the_artifact_namespace() {
    let root = Path::new("/workspace");
    let scan = GraphScan::default();
    let under = |patterns| namespace_under(&scan, root, RegistryView::installed(), patterns);
    assert_ne!(under("list-a").unwrap(), under("list-b").unwrap());
    assert_eq!(
        namespace(&scan, root, RegistryView::installed()).unwrap(),
        under(octocode_engine::security::patterns_identity()).unwrap()
    );
}
