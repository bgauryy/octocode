//! Written visibility, trait-impl heads and module placement.
use super::*;

#[test]
fn written_visibility_reads_every_restriction() {
    assert_eq!(written("fn "), Written::Module);
    assert_eq!(written("pub(self) fn "), Written::Module);
    assert_eq!(written("pub(super) fn "), Written::Up(1));
    assert_eq!(written("pub(in super::super) fn "), Written::Up(2));
    assert_eq!(written("pub(crate) async fn "), Written::Crate);
    assert_eq!(
        written("pub(in crate::a::b) fn "),
        Written::Named(vec!["a".into(), "b".into()])
    );
    assert_eq!(written("pub const fn "), Written::Public);
    assert_eq!(
        written("#[inline]\n/// doc\npub(super) fn "),
        Written::Up(1)
    );
    assert_eq!(visibility_text("pub( super ) fn "), "pub(super)");
    assert_eq!(visibility_text("fn "), "private");
}

#[test]
fn trait_impls_are_told_from_higher_ranked_bounds() {
    assert!(implements_trait("impl Display for Registry "));
    assert!(implements_trait(
        "impl<T> From<T> for Registry<T>\nwhere T: Copy "
    ));
    assert!(!implements_trait(
        "impl<F: for<'a> Fn(&'a u8)> Registry<F> "
    ));
    assert!(!implements_trait("impl Registry "));
}

#[test]
fn visibility_places_as_the_module_layout_reads() {
    let package = Path::new("/w/alpha");
    let file = Path::new("/w/alpha/src/engine/cursors.rs");
    let at = |written: Written| place(&written, file, package, &[]);
    assert_eq!(
        at(Written::Module),
        Bound::Territory("/w/alpha/src/engine/cursors".into())
    );
    assert_eq!(
        at(Written::Up(1)),
        Bound::Territory("/w/alpha/src/engine".into())
    );
    assert_eq!(at(Written::Up(9)), Bound::Crate);
    assert_eq!(
        at(Written::Named(vec!["engine".into()])),
        Bound::Territory("/w/alpha/src/engine".into())
    );
    // A crate root and `mod.rs` own their directory.
    let root = place(
        &Written::Module,
        Path::new("/w/alpha/src/lib.rs"),
        package,
        &[],
    );
    assert_eq!(root, Bound::Territory("/w/alpha/src".into()));
    assert_eq!(
        narrower(
            Bound::Territory("/w/a".into()),
            Bound::Territory("/w/a/b".into())
        ),
        Bound::Territory("/w/a/b".into())
    );
    assert_eq!(narrower(Bound::Public, Bound::Crate), Bound::Crate);
    assert!(in_territory(
        Path::new("/w/alpha/src/engine.rs"),
        Path::new("/w/alpha/src/engine")
    ));
    assert!(!in_territory(
        Path::new("/w/alpha/src/engines.rs"),
        Path::new("/w/alpha/src/engine")
    ));
}
