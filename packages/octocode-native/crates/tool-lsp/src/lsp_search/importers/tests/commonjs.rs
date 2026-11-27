//! A CommonJS anchor's importers name its module: files that bind the name
//! from another module, and markup that only spells it, are no importers.
use super::*;

const ANCHOR: &str = "function stage(a) {\n  return a;\n}\nmodule.exports = { stage };\n";

fn write(root: &Path, name: &str, text: &str) -> String {
    let path = root.join(name);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    std::fs::write(&path, text).expect("write");
    path.to_string_lossy().into_owned()
}

/// Markup that spells the name (an element id, a script reading it) without
/// importing the anchor's module holds no use of a CommonJS export.
#[test]
fn markup_without_an_import_of_a_commonjs_anchor_is_no_importer() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("canonical");
    let anchor = write(&root, "fx/a.cjs", ANCHOR);
    let module = AnchorModule::of(&anchor, ANCHOR, "stage").expect("a CommonJS export filters");
    let page = "<section id=\"stage\"></section>\n<script>\nconst stage = document.getElementById('stage');\nstage.hidden = false;\n</script>\n";
    let html = write(&root, "page.html", page);
    assert!(!super::super::markup::uses(
        &html,
        page,
        "stage",
        Some(&module),
        &mut HashMap::new()
    ));
    let vue = "<script setup>\nimport { stage } from './fx/a.cjs';\nstage(1);\n</script>\n";
    let app = write(&root, "App.vue", vue);
    assert!(super::super::markup::uses(
        &app,
        vue,
        "stage",
        Some(&module),
        &mut HashMap::new()
    ));
}
