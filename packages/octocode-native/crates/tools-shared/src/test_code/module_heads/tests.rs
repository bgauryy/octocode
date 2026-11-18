use super::*;

/// The index answers as the text scans do, for every name the texts
/// spell, from every char boundary.
#[test]
fn lookups_equal_the_scans() {
    let names = [
        "deep", "other", "x", "deeper", "b", "é", "mod", "tail", "a-b", "", "r",
    ];
    let files = ["x.rs", "b.rs", "a.b.rs", "ab.rs", "deep.rs", "é.rs"];
    for text in TEXTS {
        let heads = ModuleHeads::of(text);
        for name in names {
            for from in (0..=text.len()).filter(|at| text.is_char_boundary(*at)) {
                assert_eq!(
                    heads.declares_module(text, name, from),
                    may_declare_module(text, name, from),
                    "{text:?} {name:?} {from}"
                );
            }
        }
        for file in files {
            assert_eq!(
                heads.names_file(text, file),
                word_at(text, file, 0).is_some(),
                "{text:?} {file:?}"
            );
        }
    }
}
