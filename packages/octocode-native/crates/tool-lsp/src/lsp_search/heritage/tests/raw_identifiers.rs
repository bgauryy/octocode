use super::*;

#[test]
fn raw_rust_trait_names_match_subtypes_under_both_spellings() {
    let source = "trait r#Trait {}\nstruct S;\nimpl Trait for S {}\ntrait TraitSuffix {}\nstruct Other;\nimpl TraitSuffix for Other {}\n";
    let file = absolute("lib.rs");
    for name in ["Trait", "r#Trait"] {
        assert_eq!(scannable(source, &file, 0, name), Some(Scannable::Subtypes));
        let found = subtype_snippets(source, &file, name);
        assert_eq!(found.len(), 1, "{name}: {found:?}");
        assert_eq!(found[0].range.start.line, 2, "{name}: {found:?}");
    }
}

#[test]
fn raw_rust_trait_members_match_implementations_under_both_spellings() {
    let source = "trait r#Trait { fn r#type(&self); }\nstruct S;\nimpl r#Trait for S { fn r#type(&self) {} }\n";
    let file = absolute("lib.rs");
    for base in ["Trait", "r#Trait"] {
        for member in ["type", "r#type"] {
            assert_eq!(
                scannable(source, &file, 0, member),
                Some(Scannable::Members {
                    owner: "r#Trait".to_owned()
                })
            );
            let found = member_snippets(source, &file, base, member);
            assert_eq!(found.len(), 1, "{base} {member}: {found:?}");
            assert_eq!(found[0].range.start.line, 2, "{base} {member}: {found:?}");
        }
    }
}
