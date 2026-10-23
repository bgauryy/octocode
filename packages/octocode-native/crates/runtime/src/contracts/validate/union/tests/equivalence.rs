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

/// Output validation as `validate_output` runs it, keeping the
/// normalized copy so defaults chosen by union branches compare too.
fn validate_output_normalized(
    tool: &str,
    output: &Value,
) -> Result<Value, super::super::ContractValidationError> {
    let schema = &super::super::super::contract_tool(tool)?.1["outputSchema"];
    let mut candidate = output.clone();
    super::super::validate_schema(schema, schema, &mut candidate, &mut Vec::new())?;
    Ok(candidate)
}

/// The copy-free validation the response stage runs returns what
/// `validate_output` returns and leaves the response byte-identical.
fn assert_in_place_matches(tool: &str, output: &Value) {
    let mut in_place = output.clone();
    assert_eq!(
        crate::contracts::validate_output_in_place(tool, &mut in_place),
        crate::contracts::validate_output(tool, output),
        "{tool}: {output}"
    );
    assert_eq!(
        serde_json::to_string(&in_place).expect("json"),
        serde_json::to_string(output).expect("json"),
        "{tool}"
    );
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

/// The same on output envelopes, where a row's continuation is the
/// 16-branch `ExecutableContinuation` union: every fixture query and a
/// sixth of its variants as a continuation in its tool's output, some
/// under a wrong tool name; each tool's first query in every output.
/// `stride` samples the variants further (see [`SAMPLE`]).
fn compare_continuation_outputs(stride: usize) {
    let tools: Vec<&str> = crate::contracts::parsed_contract().expect("contract")["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    let envelope = |continuation: Value| {
        json!({"results":[
            {"index":0,"data":{"path":"a.txt","content":"x\n","totalLines":1,
                "next":{"continueWindow":continuation.clone()}}},
            {"index":1,"status":"error","data":{"error":"Not found","errorCode":"notFound",
                "next":{"retry":continuation}}}
        ]})
    };
    // Collect every (output tool, envelope) pair, then compare them on all
    // cores: the reference algorithm is slow on this union, and each
    // worker's `REFERENCE_ONLY` flag is its own thread-local.
    let mut cases: Vec<(String, Value)> = Vec::new();
    let mut seen = Vec::new();
    for (ordinal, fixture) in fixtures().iter().enumerate() {
        let tool = fixture["tool"].as_str().expect("tool").to_owned();
        let Some(query) = fixture["input"]["queries"].get(0) else {
            continue;
        };
        let unchanged = envelope(json!({"tool":tool,"query":{"queries":[query]}}));
        let output_tools = if seen.contains(&tool) {
            vec![tool.as_str()]
        } else {
            tools.clone()
        };
        for (index, output_tool) in output_tools.into_iter().enumerate() {
            if output_tool == tool || sampled(ordinal, index, stride) {
                cases.push((output_tool.to_owned(), unchanged.clone()));
            }
        }
        for (index, variant) in query_variants(query).into_iter().enumerate().step_by(6) {
            if index > 0 && !sampled(ordinal, index / 6, stride) {
                continue;
            }
            let named = if index % 7 == 6 { "localFetch" } else { &tool };
            let output = envelope(json!({"tool":named,"query":{"queries":[variant]}}));
            cases.push((tool.clone(), output));
        }
        seen.push(tool);
    }
    let compare = |(output_tool, output): &(String, Value)| {
        let (fast, reference) = both(|| validate_output_normalized(output_tool, output));
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
        let handles: Vec<_> = (0..crate::tools::workers::host_cores())
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
    let compared = cases.len();
    eprintln!("compared {compared} outputs, {rejected} rejected");
    assert!(
        compared * stride > 600 && rejected * stride > 200,
        "{compared}/{rejected}"
    );
}

#[test]
fn fast_union_acceptance_matches_the_reference_on_sampled_continuation_outputs() {
    compare_continuation_outputs(SAMPLE);
}

#[test]
#[ignore = "full matrix: cargo test -- --ignored full_matrix"]
fn fast_union_acceptance_matches_the_reference_on_continuation_outputs_full_matrix() {
    compare_continuation_outputs(1);
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
        crate::contracts::validate_output(tool, &output).unwrap_or_else(|error| {
            panic!("{name}: unmodified {tool} output was rejected: {error:?}")
        });
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
    assert!(compared > 0);
    eprintln!("compared {compared} recorded outputs and variants");
}
