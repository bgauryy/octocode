use super::*;

/// Runs `check` with the fast acceptance path and again with only the
/// reference algorithm, and returns both results.
fn both<T>(check: impl Fn() -> T) -> (T, T) {
    let fast = check();
    super::super::REFERENCE_ONLY.with(|flag| flag.set(true));
    let reference = check();
    super::super::REFERENCE_ONLY.with(|flag| flag.set(false));
    (fast, reference)
}

/// Run the public output checks, then keep a separately normalized copy
/// so defaults chosen by union branches compare too.
fn validate_output_normalized(
    tool: &str,
    output: &Value,
) -> Result<Value, super::super::ContractValidationError> {
    crate::contracts::validate_output(tool, output)?;
    let schema = &super::super::super::contract_tool(tool)?.1["outputSchema"];
    let mut candidate = output.clone();
    super::super::validate_schema(schema, schema, &mut candidate, &mut Vec::new())?;
    Ok(candidate)
}

/// The copy-free validation the response stage runs returns what
/// `validate_output` and the copying output mode return, and leaves the
/// response byte-identical.
fn assert_in_place_matches(tool: &str, output: &Value) {
    let mut in_place = output.clone();
    let verdict = crate::contracts::validate_output_in_place(tool, &mut in_place);
    assert_eq!(
        verdict,
        crate::contracts::validate_output(tool, output),
        "{tool}: {output}"
    );
    assert_eq!(
        verdict,
        validate_output_normalized(tool, output).map(|_| ()),
        "read-only and copying validation differ: {tool}: {output}"
    );
    assert_eq!(
        serde_json::to_string(&in_place).expect("json"),
        serde_json::to_string(output).expect("json"),
        "{tool}"
    );
}

#[test]
fn output_equivalence_preserves_the_public_undeclared_field_check() {
    let output = json!({"results":[{"index":0,"data":{
        "operation":"hover",
        "path":"source.ts",
        "lsp":{"serverAvailable":true,"provider":"hoverProvider"},
        "hover":{"contents":{"kind":"plaintext","value":"function example(): void"}}
    }}]});
    assert_eq!(
        crate::contracts::validate_output("lspSearch", &output),
        Ok(())
    );
    let (fast, reference) = both(|| validate_output_normalized("lspSearch", &output));
    assert_eq!(fast, Ok(output.clone()));
    assert_eq!(reference, fast);
    assert_in_place_matches("lspSearch", &output);

    let mut undeclared = output;
    undeclared["results"][0]["data"]["lsp"]["tool"] = json!("nope");
    let expected = crate::contracts::validate_output("lspSearch", &undeclared)
        .expect_err("the nested undeclared field must be rejected");
    assert_eq!(expected.issues.len(), 1);
    assert_eq!(expected.issues[0].rule_id, "undeclaredOutputField");
    assert_eq!(expected.issues[0].path, ["results", "data", "lsp", "tool"]);
    let (fast, reference) = both(|| validate_output_normalized("lspSearch", &undeclared));
    assert_eq!(fast, Err(expected.clone()));
    assert_eq!(reference, Err(expected));
    assert_in_place_matches("lspSearch", &undeclared);
}

fn fixtures() -> Vec<Value> {
    let fixtures: Value = serde_json::from_str(crate::contracts::generated::CONTRACT_FIXTURES_JSON)
        .expect("generated fixtures");
    fixtures.as_array().expect("fixture array").clone()
}

/// Each fixture query, then one variant per field: removed, a boolean,
/// an unknown literal, an object; plus an unknown field. The variants
/// reach every union failure diagnosis (selector mismatch, missing field,
/// mixed forms, widened literals, sibling annotations).
fn query_variants(query: &Value) -> Vec<Value> {
    let mut variants = vec![query.clone()];
    let Some(object) = query.as_object() else {
        return variants;
    };
    for key in object.keys() {
        let mut removed = object.clone();
        removed.remove(key);
        variants.push(Value::Object(removed));
        for replacement in [json!(true), json!("zz-not-a-value"), json!({}), json!(7)] {
            let mut changed = object.clone();
            changed.insert(key.clone(), replacement);
            variants.push(Value::Object(changed));
        }
    }
    let mut unknown = object.clone();
    unknown.insert("bogusField".into(), json!(1));
    variants.push(Value::Object(unknown));
    variants
}

/// The default run compares one in [`SAMPLE`] of the variant matrices
/// below (a fixed, rotating sample, so each fixture contributes and every
/// variant kind is reached across fixtures); the `*_full_matrix` tests
/// (ignored: `cargo test -- --ignored full_matrix`) compare all of it.
const SAMPLE: usize = 4;

/// Whether variant `index` of fixture `ordinal` is in a `stride` sample;
/// the unchanged query (variant 0) always is.
fn sampled(ordinal: usize, index: usize, stride: usize) -> bool {
    index == 0 || (index + ordinal).is_multiple_of(stride)
}

/// The discriminator-first acceptance path returns exactly what
/// the reference algorithm returns (same normalized value, same issues)
/// for every generated fixture and its field variants, as input.
fn compare_input_fixtures(stride: usize) {
    let mut compared = 0;
    let mut rejected = 0;
    for (ordinal, fixture) in fixtures().iter().enumerate() {
        let tool = fixture["tool"].as_str().expect("tool");
        let Some(query) = fixture["input"]["queries"].get(0) else {
            continue;
        };
        for (index, variant) in query_variants(query).into_iter().enumerate() {
            if !sampled(ordinal, index, stride) {
                continue;
            }
            let input = json!({"queries":[variant]});
            let (fast, reference) = both(|| prepare_many_and_validate(tool, input.clone()));
            assert_eq!(fast, reference, "{}: {input}", fixture["id"]);
            compared += 1;
            rejected += usize::from(fast.is_err());
        }
    }
    assert!(
        compared * stride > 1_000 && rejected * stride > 500,
        "{compared}/{rejected}"
    );
}

#[test]
fn fast_union_acceptance_matches_the_reference_on_sampled_input_fixtures() {
    compare_input_fixtures(SAMPLE);
}

#[test]
#[ignore = "full matrix: cargo test -- --ignored full_matrix"]
fn fast_union_acceptance_matches_the_reference_on_every_input_fixture_full_matrix() {
    compare_input_fixtures(1);
}

fn output_with_call(call: Value, related: bool) -> Value {
    let mut data = json!({"error":"Not found","errorCode":"notFound"});
    if related {
        data["related"] = json!([call]);
    } else {
        data["next"] = json!({"retry":call});
    }
    json!({"results":[{"index":0,"status":"error","data":data}]})
}

/// Accepted core fixtures supply authored related calls and opaque cursor
/// continuations. Error rows carry either kind for every output tool without
/// inventing another tool's successful data. Every case has an independent
/// expected verdict, so two validators rejecting everything cannot pass.
#[test]
fn fast_union_acceptance_matches_the_reference_on_current_output_envelopes() {
    let tools: Vec<&str> = crate::contracts::parsed_contract().expect("contract")["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    let mut cases: Vec<(String, Value, bool)> = Vec::new();
    let mut seen = Vec::new();
    for fixture in fixtures()
        .iter()
        .filter(|fixture| fixture["accepted"] == true)
    {
        let tool = fixture["tool"].as_str().expect("tool").to_owned();
        let related = fixture["input"].get("cursor").is_none();
        let kind = if related { "related" } else { "continue" };
        let call_path = if related {
            "/results/0/data/related/0"
        } else {
            "/results/0/data/next/retry"
        };
        let call = json!({"kind":kind,"tool":tool,"input":fixture["input"]});
        let output = output_with_call(call, related);
        cases.push((tool.clone(), output.clone(), true));

        let mut invalid_input = output.clone();
        let input = &mut invalid_input.pointer_mut(call_path).expect("call")["input"];
        if related {
            input["queries"][0]["unexpectedField"] = json!(true);
        } else {
            input["cursor"] = json!("");
        }
        cases.push((tool.clone(), invalid_input, false));

        if seen.contains(&(tool.clone(), related)) {
            continue;
        }
        seen.push((tool.clone(), related));
        for field in ["kind", "tool", "input"] {
            let mut missing = output.clone();
            missing
                .pointer_mut(call_path)
                .expect("call")
                .as_object_mut()
                .expect("object")
                .remove(field);
            cases.push((tool.clone(), missing, false));
        }
        for (field, value) in [
            ("kind", json!(if related { "continue" } else { "related" })),
            ("tool", json!("unknownTool")),
            ("input", json!({})),
            ("unexpectedField", json!(true)),
        ] {
            let mut malformed = output.clone();
            malformed.pointer_mut(call_path).expect("call")[field] = value;
            cases.push((tool.clone(), malformed, false));
        }

        // A row can lead to another tool, but a whole-response cursor must
        // return to the tool whose response was saved.
        let other =
            tools[(tools.iter().position(|name| *name == tool).expect("tool") + 1) % tools.len()];
        cases.push((other.to_owned(), output, true));
        if !related {
            let mut page = json!({"results":[],"responsePagination":{"hasMore":true,
                "next":{"kind":"continue","tool":tool,"input":fixture["input"]}}});
            cases.push((tool.clone(), page.clone(), true));
            page["responsePagination"]["next"]["tool"] = json!(other);
            cases.push((tool.clone(), page, false));
        }
    }
    for tool in &tools {
        for related in [false, true] {
            assert!(
                seen.contains(&(tool.to_string(), related)),
                "missing {tool} call kind: related={related}"
            );
        }
    }
    let compare = |(output_tool, output, expected): &(String, Value, bool)| {
        let (fast, reference) = both(|| validate_output_normalized(output_tool, output));
        assert_eq!(fast.is_ok(), *expected, "{output_tool}: {output}: {fast:?}");
        assert_eq!(fast, reference, "{output_tool}: {output}");
        assert_in_place_matches(output_tool, output);
        fast.is_err()
    };
    // Workers take the next case from one counter: a rejected envelope costs
    // up to a thousand times an accepted one and they cluster by fixture, so
    // fixed chunks kept one worker busy long after the others had finished.
    let next = AtomicUsize::new(0);
    let work = || {
        let mut rejected = 0;
        while let Some(case) = cases.get(next.fetch_add(1, Ordering::Relaxed)) {
            rejected += usize::from(compare(case));
        }
        rejected
    };
    let rejected: usize = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..octocode_engine::search::cpu::host_cores())
            .map(|_| scope.spawn(work))
            .collect();
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            })
            .sum()
    });
    eprintln!(
        "validated {} current outputs: {} accepted, {rejected} rejected",
        cases.len(),
        cases.len() - rejected
    );
}

/// Recorded responses (`OCTOCODE_RECORDED_OUTPUTS`: a directory
/// of `<n>-<tool>-<surface>.json` envelopes captured from the live tools)
/// validate identically on both paths and in place, unchanged and with
/// each row's fields individually corrupted.
#[test]
#[ignore = "needs a recorded corpus: OCTOCODE_RECORDED_OUTPUTS=<dir>"]
fn fast_union_acceptance_matches_the_reference_on_recorded_outputs() {
    let dir = std::env::var("OCTOCODE_RECORDED_OUTPUTS").expect("OCTOCODE_RECORDED_OUTPUTS");
    let mut compared = 0;
    let mut invalid_baselines = Vec::new();
    for entry in std::fs::read_dir(dir).expect("corpus dir") {
        let path = entry.expect("entry").path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("name")
            .to_owned();
        let Some(tool) = name.split('-').nth(1) else {
            continue;
        };
        let output: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        let output = output.get("structuredContent").cloned().unwrap_or(output);
        if output.get("thrown").is_some() {
            continue;
        }
        if let Err(error) = crate::contracts::validate_output(tool, &output) {
            invalid_baselines.push(format!(
                "{name}: unmodified {tool} output was rejected: {error:?}"
            ));
            continue;
        }
        let mut variants = vec![output.clone()];
        for (row_index, row) in output["results"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            for field in row["data"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(k, _)| k)
            {
                for replacement in [json!(true), json!({"tool":"nope"}), json!([1])] {
                    let mut changed = output.clone();
                    changed["results"][row_index]["data"][field] = replacement;
                    variants.push(changed);
                }
            }
        }
        for variant in variants {
            let (fast, reference) = both(|| validate_output_normalized(tool, &variant));
            assert_eq!(fast, reference, "{name}");
            assert_in_place_matches(tool, &variant);
            compared += 1;
        }
    }
    assert!(
        invalid_baselines.is_empty(),
        "{}",
        invalid_baselines.join("\n")
    );
    assert!(compared > 0);
    eprintln!("compared {compared} recorded outputs and variants");
}
