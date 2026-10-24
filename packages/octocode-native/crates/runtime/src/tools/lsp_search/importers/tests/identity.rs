//! Site identity: what the name at a candidate site binds.
use super::*;

fn set(items: &[&str]) -> HashSet<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

fn never_missing(_: &Path) -> bool {
    false
}

const DECLARATION: &str = "/r/a.ts:0:16:0:22";

fn bound(resolved: &[&str], content: &str) -> Binding {
    binding(
        &set(resolved),
        "/r/b.ts",
        &set(&[DECLARATION]),
        content,
        &never_missing,
    )
}

/// The anchor's declaration alone, or with the anchor module's own export
/// of it, is the anchor; a chain missing it is another declaration.
#[test]
fn a_site_binding_only_the_anchor_or_its_module_is_a_reference() {
    assert_eq!(bound(&[DECLARATION], ""), Binding::Anchor);
    let exported = "/r/a.ts:3:19:3:25";
    assert_eq!(bound(&[DECLARATION, exported], ""), Binding::Anchor);
    assert_eq!(bound(&["/r/other.ts:0:16:0:21"], ""), Binding::Other);
}

/// A site that also binds the file's own same-named declaration is
/// shadowed; one that also binds an import of the file the server did not
/// follow, or a declaration in a third file, is undecided.
#[test]
fn a_site_binding_another_declaration_too_is_shadowed_or_undecided() {
    let mocking = "const target = vi.fn();\nvi.mock(import('./a'), () => ({ target }));\n";
    assert_eq!(
        bound(&[DECLARATION, "/r/b.ts:0:6:0:12"], mocking),
        Binding::Other
    );
    let importing = "import { target } from './c';\nexport default { target };\n";
    assert_eq!(
        bound(&[DECLARATION, "/r/b.ts:0:9:0:15"], importing),
        Binding::Undecided
    );
    assert_eq!(
        bound(&[DECLARATION, "/r/other.ts:0:16:0:21"], ""),
        Binding::Undecided
    );
}
