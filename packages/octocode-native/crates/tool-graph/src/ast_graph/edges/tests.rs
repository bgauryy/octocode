use super::*;

/// Every kind the builder emits is classified once: the table names no
/// kind twice, and the builder's own kind literals are in it.
#[test]
fn every_emitted_kind_is_classified_once() {
    let mut names = KINDS.iter().map(|(name, _)| *name).collect::<Vec<_>>();
    let count = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), count, "a kind is listed twice");
    let builder = include_str!("../graph.rs");
    for literal in builder
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|text| text.ends_with("-import") || text.ends_with("-reexport"))
    {
        assert!(names.contains(&literal), "{literal} is not classified");
    }
    assert_eq!(load("static-import"), Load::Eager);
    assert_eq!(load(PYTHON_LAZY_IMPORT), Load::Deferred);
    assert_eq!(load(RUST_INCLUDE_DATA), Load::Data);
    assert_eq!(load(RUST_INCLUDE), Load::CompileTime);
    assert_eq!(load(RECOVERED_IMPORT), Load::Unknown);
    assert_eq!(load(TYPE_USE_IMPORT), Load::Unknown);
    assert_eq!(load(ELIDED_IMPORT), Load::TypeOnly);
    assert!(mounts(RUST_MODULE) && mounts(RUST_INCLUDE) && !mounts("rust-use"));
    assert_eq!(load("unparsed-target"), Load::CompileTime);
}
