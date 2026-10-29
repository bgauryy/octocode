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
