use super::*;

#[test]
fn every_argument_adapter_and_alias_has_reference_and_value_controls() {
    for entry in languages::all_entries() {
        let capability = entry.argument_capabilities();
        let Some(adapter) = entry.argument_adapter else {
            assert!(!capability.supported);
            assert!(capability.forms.is_empty());
            assert!(!capability.unsupported_constructs.is_empty());
            continue;
        };
        assert!(entry.fact_capabilities.calls);
        assert!(capability.supported);
        let source = positive(adapter);
        for extension in entry.extensions {
            let result = facts(source, extension);
            let observations = slots(&result);
            assert!(observations.iter().any(|slot| matches!(&slot.observation, GraphArgumentObservation::Reference { target_name, .. } if target_name == "handler")), "{extension}: {result:#?}");
            assert!(
                observations.iter().any(|slot| matches!(
                    slot.observation,
                    GraphArgumentObservation::NonReference {
                        value_kind: GraphArgumentValueKind::Literal
                    }
                )),
                "{extension}"
            );
            assert!(
                observations.iter().any(|slot| matches!(
                    slot.observation,
                    GraphArgumentObservation::NonReference {
                        value_kind: GraphArgumentValueKind::CallResult
                    }
                )),
                "{extension}"
            );
            for slot in observations {
                assert!(
                    !source[slot.source_range.start_byte as usize
                        ..slot.source_range.end_byte as usize]
                        .is_empty()
                );
                if let GraphArgumentObservation::Reference {
                    selection_byte_range,
                    ..
                } = &slot.observation
                {
                    assert_eq!(
                        &source[selection_byte_range.start_byte as usize
                            ..selection_byte_range.end_byte as usize],
                        "handler"
                    );
                }
            }
        }
    }
}

#[test]
fn direct_addresses_borrows_members_and_method_references_keep_selected_names() {
    let cases = [
        (
            "cpp",
            "void f(){ consume(&Worker::handler); }",
            "Worker::handler",
            GraphArgumentForm::AddressOf,
        ),
        (
            "cu",
            "void f(){ consume(&Worker::handler); }",
            "Worker::handler",
            GraphArgumentForm::AddressOf,
        ),
        (
            "rs",
            "fn f(){ consume(&module::handler); }",
            "module::handler",
            GraphArgumentForm::Borrow,
        ),
        (
            "go",
            "package p\nfunc f(){ consume(&handler) }",
            "handler",
            GraphArgumentForm::AddressOf,
        ),
        (
            "java",
            "class Box { void f(){ consume(Worker::handler); } }",
            "Worker::handler",
            GraphArgumentForm::MethodReference,
        ),
        (
            "py",
            "consume(module.handler)",
            "module.handler",
            GraphArgumentForm::Member,
        ),
    ];
    for (extension, source, target, form) in cases {
        if languages::find_entry(extension).is_none() {
            continue;
        }
        let result = facts(source, extension);
        assert!(slots(&result).iter().any(|slot| matches!(&slot.observation, GraphArgumentObservation::Reference { target_name, form: actual, selection_byte_range, .. } if target_name == target && *actual == form && &source[selection_byte_range.start_byte as usize..selection_byte_range.end_byte as usize] == "handler")), "{extension}: {result:#?}");
    }
}

#[test]
fn unsupported_slots_and_groups_preserve_ranges_without_inventing_names() {
    for (extension, source) in [
        ("ts", "consume(...handlers, registry[key]); tag`handler`;"),
        (
            "py",
            "consume(*handlers, **mapping)\nconsume(x for x in values)",
        ),
        ("rs", "fn f(){ consume!(handler); consume(value[0]); }"),
        (
            "scala",
            "object B { def f() = { consume { handler }; consume(values*) } }",
        ),
    ] {
        if languages::find_entry(extension).is_none() {
            continue;
        }
        let result = facts(source, extension);
        assert!(
            result
                .argument_sites
                .iter()
                .flat_map(|s| &s.groups)
                .any(|g| matches!(
                    g.observation,
                    GraphArgumentGroupObservation::Unsupported { .. }
                )),
            "{extension}: {result:#?}"
        );
        assert!(
            slots(&result).iter().any(|slot| matches!(
                slot.observation,
                GraphArgumentObservation::Unsupported { .. }
            )),
            "{extension}: {result:#?}"
        );
    }
}

#[test]
fn every_adapter_retains_unsupported_slots_and_blocks_parameter_fallback() {
    use super::super::ArgumentAdapter as A;
    for entry in languages::all_entries() {
        let Some(adapter) = entry.argument_adapter else {
            continue;
        };
        let source = match adapter {
            A::JavaScript => {
                "function handler(){} function f(handler){ consume(handler, ...items); }"
            }
            A::Python => "def handler(): pass\ndef f(handler):\n consume(handler, *items)\n",
            #[cfg(any(feature = "tree-sitter-cpp", feature = "tree-sitter-cuda"))]
            A::CFamily => "void handler(){} void f(int handler){ consume(handler, items[0]); }",
            A::C => "void handler(){} void f(int handler){ consume(handler, items[0]); }",
            A::Go => {
                "package p\nfunc handler(){}\nfunc f(handler int){ consume(handler, items...) }"
            }
            A::Java => {
                "class B { void handler(){} void f(int handler){ consume(handler, items[0]); } }"
            }
            A::Rust => "fn handler(){} fn f(handler: i32){ consume(handler, items[0]); }",
            #[cfg(feature = "tree-sitter-c-sharp")]
            A::CSharp => {
                "class B { void handler(){} void f(int handler){ consume(handler, out var item); } }"
            }
            A::Scala => {
                "object B { def handler() = (); def f(handler: Int) = consume(handler, items*) }"
            }
        };
        for extension in entry.extensions {
            let result = facts(source, extension);
            let observed = slots(&result);
            assert!(
                observed.iter().any(|slot| matches!(
                    slot.observation,
                    GraphArgumentObservation::Unsupported { .. }
                )),
                "{extension}: {result:#?}"
            );
            let references = observed
                .iter()
                .filter_map(|slot| match &slot.observation {
                    GraphArgumentObservation::Reference {
                        binding,
                        target_name,
                        ..
                    } if target_name == "handler" => Some(binding),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(references.len(), 1, "{extension}: {result:#?}");
            assert!(
                matches!(
                    references[0],
                    GraphArgumentBinding::Unrepresented { .. } | GraphArgumentBinding::NotAssessed
                ),
                "{extension}: {references:#?}"
            );
        }
    }
}

#[test]
fn advertised_member_and_qualified_forms_have_positive_controls() {
    use super::super::ArgumentAdapter as A;
    for entry in languages::all_entries() {
        let Some(adapter) = entry.argument_adapter else {
            continue;
        };
        let source = match adapter {
            A::JavaScript => "consume(space.handler)",
            A::Python => "consume(space.handler)",
            #[cfg(any(feature = "tree-sitter-cpp", feature = "tree-sitter-cuda"))]
            A::CFamily => "void f(){ consume(space.handler); }",
            A::C => "void f(){ consume(space.handler); }",
            A::Go => "package p\nfunc f(){ consume(space.handler) }",
            A::Java => "class B { void f(){ consume(space.handler); } }",
            A::Rust => "fn f(){ consume(space.handler); }",
            #[cfg(feature = "tree-sitter-c-sharp")]
            A::CSharp => "class B { void f(){ consume(space.handler); } }",
            A::Scala => "object B { def f() = consume(space.handler) }",
        };
        let result = facts(source, entry.extensions[0]);
        assert!(slots(&result).iter().any(|slot| matches!(&slot.observation, GraphArgumentObservation::Reference { target_name, form: GraphArgumentForm::Member, .. } if target_name == "space.handler")), "{}: {result:#?}", entry.name);
        if entry
            .argument_capabilities()
            .forms
            .contains(&GraphArgumentForm::Qualified)
        {
            let source = if adapter == A::Rust {
                "fn f(){ consume(space::handler); }"
            } else {
                "void f(){ consume(space::handler); }"
            };
            let result = facts(source, entry.extensions[0]);
            assert!(slots(&result).iter().any(|slot| matches!(&slot.observation, GraphArgumentObservation::Reference { target_name, form: GraphArgumentForm::Qualified, .. } if target_name == "space::handler")), "{}: {result:#?}", entry.name);
        }
    }
}
