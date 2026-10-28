use super::*;

#[test]
fn oxc_binding_ids_block_parameters_destructuring_catch_and_loop_shadows() {
    let source = "function handler(){} function parameter(handler){ consume(handler); } function local(){ consume(handler); const handler = other; consume(handler); } function destructured({handler}){ consume(handler); } try {} catch(handler){ consume(handler); } for (const handler of values) { consume(handler); } consume(handler);";
    let result = facts(source, "ts");
    let global = result
        .declarations
        .iter()
        .find(|d| d.name == "handler" && d.kind == "function" && d.parent.is_none())
        .expect("global");
    let bindings = slots(&result)
        .into_iter()
        .filter_map(|slot| match &slot.observation {
            GraphArgumentObservation::Reference { binding, .. } => Some(binding),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(bindings.len(), 7);
    assert!(bindings[..6].iter().all(|binding| !matches!(binding, GraphArgumentBinding::ScopedLookup { .. }) && !matches!(binding, GraphArgumentBinding::Declaration { declaration_id } if declaration_id == &global.id)), "{bindings:#?}");
    assert_eq!(
        bindings[6],
        &GraphArgumentBinding::Declaration {
            declaration_id: global.id.clone()
        }
    );
}

#[test]
fn python_whole_function_assignment_and_parameters_block_global_lookup() {
    let result = facts(
        "def handler(): pass\ndef parameter(handler):\n consume(handler)\ndef local():\n consume(handler)\n handler = other\n consume(handler)\nconsume(handler)\n",
        "py",
    );
    let bindings = slots(&result)
        .into_iter()
        .filter_map(|slot| match &slot.observation {
            GraphArgumentObservation::Reference { binding, .. } => Some(binding),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(bindings.len(), 4);
    assert!(
        bindings[..3]
            .iter()
            .all(|binding| !matches!(binding, GraphArgumentBinding::ScopedLookup { .. })),
        "{bindings:#?}"
    );
    assert!(matches!(
        bindings[3],
        GraphArgumentBinding::ScopedLookup { .. }
    ));
}

#[test]
fn rust_local_visibility_and_destructuring_do_not_escape_their_scope() {
    let result = facts(
        "fn handler(){} fn f(){ consume(handler); { let (handler, other) = pair; consume(handler); } consume(handler); }",
        "rs",
    );
    let bindings = slots(&result)
        .into_iter()
        .filter_map(|slot| match &slot.observation {
            GraphArgumentObservation::Reference { binding, .. } => Some(binding),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(bindings.len(), 3);
    assert!(matches!(
        bindings[0],
        GraphArgumentBinding::ScopedLookup { .. }
    ));
    assert!(matches!(
        bindings[1],
        GraphArgumentBinding::Unrepresented { .. } | GraphArgumentBinding::Declaration { .. }
    ));
    assert!(matches!(
        bindings[2],
        GraphArgumentBinding::ScopedLookup { .. }
    ));
}

#[test]
fn recovered_enclosure_does_not_promote_bindings_or_taint_healthy_neighbors() {
    let result = facts(
        "function broken(){ const missing; consume(handler); } function healthy(){ consume(handler); }",
        "js",
    );
    assert!(
        result.argument_sites.iter().any(|site| matches!(
            site.parse_assessment,
            GraphParseAssessment::Recovered { .. }
        )),
        "{result:#?}"
    );
    assert!(
        result
            .argument_sites
            .iter()
            .any(|site| site.caller == "healthy"
                && matches!(site.parse_assessment, GraphParseAssessment::Parsed { .. }))
    );
}

#[test]
fn dynamic_javascript_scope_is_an_explicit_binding_boundary() {
    let result = facts("function f(){ eval(source); consume(handler); }", "js");
    assert!(slots(&result).iter().any(|slot| matches!(&slot.observation, GraphArgumentObservation::Reference { target_name, binding: GraphArgumentBinding::NotAssessed, .. } if target_name == "handler")));
}

#[test]
fn loop_bindings_are_visible_in_their_body_and_do_not_fall_back_to_globals() {
    for (extension, source) in [
        (
            "rs",
            "fn handler() {} fn run(){ for handler in [1] { consume(handler); } }",
        ),
        (
            "rs",
            "fn handler() {} fn run(){ if let Some(handler) = value { consume(handler); } }",
        ),
        (
            "go",
            "package p\nfunc handler() {}\nfunc run(){ for _, handler := range values { consume(handler) } }",
        ),
        (
            "java",
            "class B { void handler(){} void run(){ for (int handler : values) { consume(handler); } } }",
        ),
    ] {
        let result = facts(source, extension);
        let binding = slots(&result)
            .into_iter()
            .find_map(|slot| match &slot.observation {
                GraphArgumentObservation::Reference {
                    target_name,
                    binding,
                    ..
                } if target_name == "handler" => Some(binding),
                _ => None,
            })
            .expect("observed reference");
        assert!(
            !matches!(binding, GraphArgumentBinding::ScopedLookup { .. }),
            "{extension}: {binding:#?}"
        );
        if let GraphArgumentBinding::Declaration { declaration_id } = binding {
            assert!(
                !result.declarations.iter().any(|d| d.id == *declaration_id
                    && d.name == "handler"
                    && d.kind == "function")
            );
        }
    }
}

#[test]
fn unsupported_lexical_binding_forms_block_global_fallback() {
    for (extension, source) in [
        (
            "py",
            "def handler(): pass\ndef run():\n consume(handler)\n del handler\n",
        ),
        (
            "py",
            "def handler(): pass\ndef run():\n from other import handler\n consume(handler)\n",
        ),
        (
            "rs",
            "fn handler() {} fn run(){ fn handler() {} consume(handler); }",
        ),
        (
            "c",
            "void handler(void){} void *identity(void *x){return x;} void run(void){ void *handler = identity(handler); }",
        ),
        (
            "cpp",
            "void handler(){} class B { void *handler; void run(){ consume(handler); } };",
        ),
    ] {
        if languages::find_entry(extension).is_none() {
            continue;
        }
        let result = facts(source, extension);
        let binding = slots(&result)
            .into_iter()
            .find_map(|slot| match &slot.observation {
                GraphArgumentObservation::Reference {
                    target_name,
                    binding,
                    ..
                } if target_name == "handler" => Some(binding),
                _ => None,
            })
            .expect("observed reference");
        assert!(
            !matches!(binding, GraphArgumentBinding::ScopedLookup { .. }),
            "{extension}: {binding:#?}"
        );
        if let GraphArgumentBinding::Declaration { declaration_id } = binding {
            assert!(
                !result.declarations.iter().any(|d| d.id == *declaration_id
                    && d.name == "handler"
                    && d.kind == "function")
            );
        }
    }
}

#[test]
fn imported_identifiers_retain_the_observed_import_binding() {
    for (extension, source) in [
        (
            "ts",
            "import {handler} from './a'; class Box { handler(){} } export function run(){ sink(handler); }",
        ),
        (
            "rs",
            "use other::handler; struct Box; impl Box { fn handler(){} } fn run(){ sink(handler); }",
        ),
    ] {
        let result = facts(source, extension);
        let reference = slots(&result).into_iter().find(|slot| matches!(&slot.observation, GraphArgumentObservation::Reference { target_name, .. } if target_name == "handler")).expect("imported reference");
        let GraphArgumentObservation::Reference {
            binding: GraphArgumentBinding::Import { import_id },
            ..
        } = &reference.observation
        else {
            panic!("{extension}: {reference:#?}")
        };
        assert!(result.imports.iter().any(
            |import| import.id == *import_id && import.local_name.as_deref() == Some("handler")
        ));
    }
}

#[test]
fn ordered_module_imports_do_not_establish_earlier_or_mutable_bindings() {
    let source = "def handler(): pass\nconsume(handler)\nfrom other import handler\nconsume(handler)\nconsume(unrelated)\n";
    let result = facts(source, "py");
    let observed = slots(&result)
        .into_iter()
        .filter_map(|slot| match &slot.observation {
            GraphArgumentObservation::Reference {
                target_name,
                binding,
                ..
            } => Some((target_name.as_str(), binding)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(observed.len(), 3);
    assert!(
        observed[..2]
            .iter()
            .all(|(name, binding)| *name == "handler"
                && matches!(binding, GraphArgumentBinding::NotAssessed)),
        "{observed:#?}"
    );
    assert!(matches!(
        observed[2],
        ("unrelated", GraphArgumentBinding::ScopedLookup { .. })
    ));
}

#[test]
fn pattern_and_catch_scope_exit_do_not_retain_the_inner_binding() {
    let source = "fn handler(){} fn run(){ if let Some(handler)=value && predicate(handler){ consume(handler); } consume(handler); }";
    let result = facts(source, "rs");
    let observed = slots(&result)
        .into_iter()
        .filter_map(|slot| match &slot.observation {
            GraphArgumentObservation::Reference {
                target_name,
                binding,
                ..
            } if target_name == "handler" => Some(binding),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(observed.len(), 3);
    assert!(matches!(observed[0], GraphArgumentBinding::NotAssessed));
    assert!(matches!(
        observed[1],
        GraphArgumentBinding::Unrepresented { .. } | GraphArgumentBinding::Declaration { .. }
    ));
    assert!(matches!(
        observed[2],
        GraphArgumentBinding::ScopedLookup {
            implicit_members: false
        }
    ));

    let source = "class Box { Object handler; void sink(Object x){} void run(){ try {} catch(RuntimeException handler){ sink(handler); } sink(handler); } }";
    let result = facts(source, "java");
    let observed = slots(&result)
        .into_iter()
        .filter_map(|slot| match &slot.observation {
            GraphArgumentObservation::Reference {
                target_name,
                binding,
                ..
            } if target_name == "handler" => Some(binding),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(observed.len(), 2);
    let catch_start =
        source.find("Exception handler").expect("catch") as u32 + "Exception ".len() as u32;
    assert!(
        matches!(observed[0], GraphArgumentBinding::Unrepresented { source_range } if source_range.start_byte == catch_start)
    );
    assert!(
        !matches!(observed[1], GraphArgumentBinding::Unrepresented { source_range } if source_range.start_byte == catch_start)
    );
    assert!(!matches!(observed[1], GraphArgumentBinding::NotAssessed));
}
