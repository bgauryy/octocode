#![allow(clippy::expect_used)]
//! High-hit parallel search benchmark: many matched files across many
//! directories, so per-file result collection (not regex evaluation) is a
//! visible share of the runtime. Guards the worker-local collection path in
//! `search/text_search.rs::collect`.

use criterion::{Criterion, criterion_group, criterion_main};
use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use octocode_engine::portable::{TextSearchPathFilter, search_text_cancellable};
use octocode_engine::types::TextSearchOptions;

struct AllowAll;
impl TextSearchPathFilter for AllowAll {
    fn allows(&self, _: &Path, _: bool) -> bool {
        true
    }
}

const FILES: usize = 3_000;
const MATCHING_LINES: usize = 20;

/// Build (once) a persistent fixture tree of `FILES` files, every one of which
/// matches the benchmark pattern.
fn fixture_tree() -> PathBuf {
    let root = std::env::temp_dir().join(format!("octocode-search-bench-{FILES}x{MATCHING_LINES}"));
    let marker = root.join(".complete");
    if marker.exists() {
        return root;
    }
    let _ = fs::remove_dir_all(&root);
    let mut body = String::new();
    for line in 0..MATCHING_LINES {
        body.push_str(&format!("const needle_{line} = {line};\n"));
        body.push_str("const other = 0;\n");
    }
    for index in 0..FILES {
        let dir = root.join(format!("d{}", index / 100));
        fs::create_dir_all(&dir).expect("bench fixture dir");
        fs::write(dir.join(format!("f{index}.ts")), &body).expect("bench fixture file");
    }
    fs::write(&marker, b"ok").expect("bench fixture marker");
    root
}

fn high_hit_search(criterion: &mut Criterion) {
    let root = fixture_tree();
    let path = root.display().to_string();
    // `_digest` hashes every searched file in the read it searches with, as
    // localSearch asks (its page cache keeps those digests).
    for (name, digest_max_bytes) in [
        ("text_high_hit_collect", None),
        ("text_high_hit_collect_digest", Some(64 * 1024 * 1024)),
    ] {
        criterion.bench_function(name, |bencher| {
            bencher.iter(|| {
                let result = search_text_cancellable(
                    TextSearchOptions {
                        path: path.clone(),
                        pattern: "needle".to_owned(),
                        fixed_string: Some(true),
                        digest_max_bytes,
                        ..Default::default()
                    },
                    Arc::new(AllowAll),
                    &|| false,
                )
                .expect("bench search");
                black_box(result)
            });
        });
    }
}

criterion_group!(benches, high_hit_search);
criterion_main!(benches);
