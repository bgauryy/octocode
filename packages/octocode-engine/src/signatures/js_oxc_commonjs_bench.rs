//! Controlled benchmark: identical graph extraction with and without CommonJS facts.
//! Run explicitly; wall-clock measurements are not correctness-test assertions.

use std::hint::black_box;
use std::time::Instant;

use super::extract_graph_facts_inner;

const TRIALS: usize = 9;
const ITERATIONS: usize = 80;

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}

fn measure<const COMMON_JS: bool>(source: &str, file: &str) -> f64 {
    let started = Instant::now();
    for _ in 0..ITERATIONS {
        black_box(extract_graph_facts_inner::<COMMON_JS>(black_box(source), file).unwrap());
    }
    started.elapsed().as_secs_f64() * 1_000_000.0 / ITERATIONS as f64
}

fn corpus() -> Vec<(&'static str, &'static str, String, usize, &'static str)> {
    let esm = (0..350).map(|i| format!("export function task{i}(input) {{ const value = transform(input); return {{ value, index: {i} }}; }}\n")).collect::<String>();
    let cjs = (0..180).map(|i| format!("const dep{i} = require('./dep{i}.cjs'); function task{i}(input) {{ return dep{i}.run(input); }}\n")).collect::<String>();
    let tsx = (0..240).map(|i| format!("export const Row{i} = (props: {{ title: string }}) => <View id={{props.title}}>{{props.title}}</View>;\n")).collect::<String>();
    let nested = (0..120).map(|i| format!(r#"function task{i}() {{ const name = './dep{i}.cjs'; return requ\u0069re(name); }}"#)).collect::<String>();
    vec![
        ("generated_esm", "fixture.mjs", esm, 0, "train"),
        (
            "repository_graph_builder",
            "fixture.ts",
            include_str!("../../../octocode-tools-core/src/graph/buildFileGraph.ts").to_owned(),
            0,
            "train",
        ),
        ("generated_commonjs", "fixture.cjs", cjs, 180, "train"),
        (
            "repository_import_resolver",
            "fixture.ts",
            include_str!("../../../octocode-tools-core/src/graph/importResolver.ts").to_owned(),
            0,
            "held-out",
        ),
        ("generated_tsx", "fixture.tsx", tsx, 0, "held-out"),
        (
            "nested_escaped_commonjs",
            "fixture.js",
            nested,
            120,
            "held-out",
        ),
    ]
}

#[test]
#[ignore = "manual controlled performance measurement; writes a workspace-local report"]
fn commonjs_graph_benchmark() {
    let split = std::env::var("OCTOCODE_BENCH_SPLIT").unwrap_or_else(|_| "train".to_owned());
    let label = std::env::var("OCTOCODE_BENCH_LABEL").unwrap_or_else(|_| "baseline".to_owned());
    assert!(label
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-'));
    let mut rows = Vec::new();
    for (name, file, source, expected_loads, case_split) in corpus() {
        if split != "all" && split != case_split {
            continue;
        }
        let mut baseline: serde_json::Value =
            serde_json::from_str(&extract_graph_facts_inner::<false>(&source, file).unwrap())
                .unwrap();
        let mut current: serde_json::Value =
            serde_json::from_str(&extract_graph_facts_inner::<true>(&source, file).unwrap())
                .unwrap();
        assert_eq!(
            current["commonJs"].as_array().unwrap().len(),
            expected_loads,
            "{name}: module-load recall guard"
        );
        baseline.as_object_mut().unwrap().remove("commonJs");
        current.as_object_mut().unwrap().remove("commonJs");
        assert_eq!(
            baseline, current,
            "{name}: unrelated graph fact parity guard"
        );
        for _ in 0..12 {
            black_box(extract_graph_facts_inner::<false>(&source, file));
            black_box(extract_graph_facts_inner::<true>(&source, file));
        }
        let mut without = Vec::new();
        let mut with = Vec::new();
        for trial in 0..TRIALS {
            if trial % 2 == 0 {
                without.push(measure::<false>(&source, file));
                with.push(measure::<true>(&source, file));
            } else {
                with.push(measure::<true>(&source, file));
                without.push(measure::<false>(&source, file));
            }
        }
        let source_hash = source.bytes().fold(0xcbf29ce484222325u64, |hash, byte| {
            (hash ^ byte as u64).wrapping_mul(0x100000001b3)
        });
        let baseline_us = median(without.clone());
        let current_us = median(with.clone());
        rows.push(serde_json::json!({ "case": name, "split": case_split, "bytes": source.len(), "sourceHash": format!("{source_hash:016x}"), "expectedLoads": expected_loads, "withoutCommonJsMedianUs": baseline_us, "withCommonJsMedianUs": current_us, "overheadPercent": (current_us / baseline_us - 1.0) * 100.0, "withoutCommonJsTrialsUs": without, "withCommonJsTrialsUs": with }));
    }
    let report = serde_json::json!({ "label": label, "split": split, "debugAssertions": cfg!(debug_assertions), "trials": TRIALS, "iterationsPerTrial": ITERATIONS, "measurement": "in-thread native graph extraction including parsing and JSON serialization; no filesystem scan or thread dispatch", "rows": rows });
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.octocode/octocode-eval-benchmark/commonjs");
    std::fs::create_dir_all(&directory).unwrap();
    let destination = directory.join(format!("{label}-{split}.json"));
    std::fs::write(&destination, serde_json::to_string_pretty(&report).unwrap()).unwrap();
}
