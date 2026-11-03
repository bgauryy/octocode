//! Release validation baseline over complete recorded tool responses.
//! `OCTOCODE_RECORDED_OUTPUTS=<directory> cargo bench -p octocode-native --bench contract_layout`.
#![expect(
    clippy::expect_used,
    reason = "benchmark fixtures must be readable and valid"
)]

use octocode_native::contracts::validate_output;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::hint::black_box;
use std::time::Instant;

fn main() {
    let dir = std::env::var("OCTOCODE_RECORDED_OUTPUTS").expect("recorded corpus directory");
    let mut cases: BTreeMap<String, (String, Value)> = BTreeMap::new();
    for entry in std::fs::read_dir(dir).expect("corpus directory") {
        let path = entry.expect("corpus entry").path();
        let name = path.file_name().expect("filename").to_string_lossy();
        let Some(tool) = name.split('-').nth(1) else {
            continue;
        };
        let text = std::fs::read_to_string(&path).expect("response");
        let mut value: Value = serde_json::from_str(&text).expect("JSON response");
        if let Some(structured) = value.get_mut("structuredContent") {
            value = structured.take();
        }
        if value.get("results").is_none() {
            continue;
        }
        // The largest complete captured envelope per tool stresses its actual
        // schema and evidence, without inventing shapes or timing file reads.
        let bytes = value.to_string().len();
        let replace = cases
            .get(tool)
            .is_none_or(|(_, known)| bytes > known.to_string().len());
        if replace {
            cases.insert(tool.to_owned(), (name.into_owned(), value));
        }
    }
    assert!(!cases.is_empty(), "empty corpus");
    for (tool, (file, value)) in cases {
        validate_output(&tool, &value).expect("valid captured envelope");
        let samples = 101;
        let mut times = Vec::with_capacity(samples);
        for _ in 0..samples {
            let start = Instant::now();
            validate_output(black_box(&tool), black_box(&value)).expect("valid envelope");
            times.push(start.elapsed().as_nanos());
        }
        times.sort_unstable();
        println!(
            "{}",
            json!({"tool":tool,"fixture":file,"bytes":value.to_string().len(),
                "samples":samples,"medianNs":times[samples / 2],"minNs":times[0],
                "p95Ns":times[samples * 95 / 100]})
        );
    }
}
