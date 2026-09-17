mod algorithms;
mod model;

pub use algorithms::{
    condense as condense_file_graph, cycle_witness, reachable as reachable_files,
    reverse as reverse_file_graph, scc as strongly_connected_components,
    scc_unsorted as strongly_connected_components_unsorted, shortest_path as shortest_file_path,
    transitive_edges, traverse as traverse_file_graph, Condensed as CondensedFileGraph,
    Node as FileGraphNode,
};
pub use model::{
    CodeEdge, CodeGraphBuilder, CodeGraphDiagnostic, CodeGraphSnapshot, CodeNode, EdgeKind,
    Evidence, EvidenceId, EvidenceSource, GraphBuildMetrics, GraphBuildReceipt, GraphCompleteness,
    GraphFactCall, GraphFactCommonJs, GraphFactDeclaration, GraphFactEdge, GraphFactExport,
    GraphFactImport, GraphFactRustModule, GraphFactsDocument, GraphFactsTypedEntry,
    GraphFactsTypedScanResult, GraphPosition, GraphRange, NodeId, NodeKind, SemanticRelationInput,
    ServerReceipt, SnapshotMetadata,
};

use std::{fs, io::Read, path::Path};

use rayon::prelude::*;

use crate::types::{
    FileSystemQueryOptions, GraphFactsScanDiagnostic, GraphFactsScanEntry, GraphFactsScanOptions,
    GraphFactsScanResult, GraphReferenceCount,
};

const DEFAULT_MAX_FILES: u32 = 20_000;
const DEFAULT_MAX_FILE_BYTES: u32 = 1_000_000;

enum GraphFactsScanOutcome {
    Entry(Box<GraphFactsTypedEntry>),
    Skipped(GraphFactsScanDiagnostic),
}

fn skipped(relative_path: String, code: &str, message: &str) -> GraphFactsScanOutcome {
    GraphFactsScanOutcome::Skipped(GraphFactsScanDiagnostic {
        relative_path,
        code: code.to_owned(),
        message: message.to_owned(),
    })
}

pub(crate) fn scan_graph_facts(
    options: GraphFactsScanOptions,
) -> Result<GraphFactsScanResult, String> {
    scan_graph_facts_filtered(options, &|_| Ok(true))
}

pub(crate) fn scan_graph_facts_filtered(
    options: GraphFactsScanOptions,
    allow_path: &(dyn Fn(&Path) -> Result<bool, String> + Sync),
) -> Result<GraphFactsScanResult, String> {
    let typed = scan_graph_facts_typed_filtered(options, allow_path)?;
    let entries = typed
        .entries
        .into_iter()
        .map(|entry| {
            let facts_json = serde_json::to_string(&entry.facts)
                .map_err(|error| format!("graph facts could not be encoded: {error}"))?;
            Ok(GraphFactsScanEntry {
                relative_path: entry.relative_path,
                facts_json,
                reference_counts: entry.reference_counts,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(GraphFactsScanResult {
        schema_version: typed.schema_version,
        entries,
        skipped: typed.skipped,
        candidate_paths: typed.candidate_paths,
        files_skipped: typed.files_skipped,
        truncated: typed.truncated,
    })
}

pub(crate) fn scan_graph_facts_typed(
    options: GraphFactsScanOptions,
) -> Result<GraphFactsTypedScanResult, String> {
    scan_graph_facts_typed_filtered(options, &|_| Ok(true))
}

pub(crate) fn scan_graph_facts_typed_filtered(
    options: GraphFactsScanOptions,
    allow_path: &(dyn Fn(&Path) -> Result<bool, String> + Sync),
) -> Result<GraphFactsTypedScanResult, String> {
    let max_files = options.max_files.unwrap_or(DEFAULT_MAX_FILES);
    let max_file_bytes = options.max_file_bytes.unwrap_or(DEFAULT_MAX_FILE_BYTES) as i64;
    let query = crate::search::fs_query::query_file_system_filtered_inner(
        FileSystemQueryOptions {
            path: options.path,
            recursive: Some(true),
            show_hidden: Some(false),
            entry_type: Some("f".to_owned()),
            extensions: Some(crate::signatures::graph_facts::graph_fact_extensions()),
            exclude_dir: options.exclude_dir,
            stop_at_limit: Some(true),
            limit: Some(max_files),
            ..Default::default()
        },
        allow_path,
    )?;

    let truncated = query.was_capped;
    let mut candidate_paths: Vec<String> = query
        .entries
        .iter()
        .map(|entry| entry.relative_path.replace('\\', "/"))
        .collect();
    candidate_paths.sort_unstable();
    let outcomes: Vec<Option<GraphFactsScanOutcome>> = query
        .entries
        .into_par_iter()
        .map(|entry| -> Result<Option<GraphFactsScanOutcome>, String> {
            let path = Path::new(&entry.path);
            if !allow_path(path)? {
                return Ok(None);
            }
            let relative_path = entry.relative_path.replace('\\', "/");
            let outcome =
                |code: &str, message: &str| Ok(Some(skipped(relative_path.clone(), code, message)));
            if entry.size.unwrap_or_default() > max_file_bytes {
                return outcome(
                    "graph.scan.fileTooLarge",
                    "file exceeds the graph scan byte limit",
                );
            }
            let Ok(file) = fs::File::open(path) else {
                return outcome(
                    "graph.scan.readFailed",
                    "file could not be read as UTF-8 text",
                );
            };
            // Metadata is advisory: a file can grow between discovery and open.
            // The read itself has a strict bound, including one overflow byte.
            let mut content = String::new();
            if file
                .take(max_file_bytes as u64 + 1)
                .read_to_string(&mut content)
                .is_err()
            {
                return outcome(
                    "graph.scan.readFailed",
                    "file could not be read as UTF-8 text",
                );
            }
            if content.len() > max_file_bytes as usize {
                return outcome(
                    "graph.scan.fileTooLarge",
                    "file exceeds the graph scan byte limit",
                );
            }
            if !allow_path(path)? {
                return Ok(None);
            }
            let Some(extraction) = crate::signatures::extract_graph_facts_with_metadata_inner(
                &content,
                &relative_path,
            ) else {
                return outcome(
                    "graph.scan.extractFailed",
                    "native graph-fact extraction returned no result",
                );
            };
            if !allow_path(path)? {
                return Ok(None);
            }
            let reference_counts =
                exported_reference_counts(&content, &extraction.exported_declaration_names);
            Ok(Some(GraphFactsScanOutcome::Entry(Box::new(
                GraphFactsTypedEntry {
                    relative_path,
                    content_digest: crate::index::content_digest(content.as_bytes()),
                    facts: extraction.facts,
                    reference_counts,
                },
            ))))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut entries = Vec::new();
    let mut skipped = Vec::new();
    for outcome in outcomes.into_iter().flatten() {
        match outcome {
            GraphFactsScanOutcome::Entry(entry) => entries.push(*entry),
            GraphFactsScanOutcome::Skipped(diagnostic) => skipped.push(diagnostic),
        }
    }
    entries.sort_unstable_by(|left, right| left.relative_path.cmp(&right.relative_path));
    skipped.sort_unstable_by(|left, right| left.relative_path.cmp(&right.relative_path));
    let files_skipped = skipped.len() as u32;

    Ok(GraphFactsTypedScanResult {
        schema_version: crate::signatures::GRAPH_FACTS_SCHEMA_VERSION,
        entries,
        skipped,
        candidate_paths,
        files_skipped,
        truncated,
    })
}

fn exported_reference_counts(
    content: &str,
    exported_declaration_names: &[String],
) -> Vec<GraphReferenceCount> {
    exported_declaration_names
        .iter()
        .map(|name| GraphReferenceCount {
            name: name.clone(),
            count: count_ascii_word_occurrences(content, name),
        })
        .collect()
}

fn count_ascii_word_occurrences(content: &str, name: &str) -> u32 {
    if name.is_empty() {
        return 0;
    }
    content
        .match_indices(name)
        .filter(|(start, _)| {
            let end = start + name.len();
            let before_is_word = content[..*start]
                .chars()
                .next_back()
                .is_some_and(is_ascii_word_char);
            let after_is_word = content[end..]
                .chars()
                .next()
                .is_some_and(is_ascii_word_char);
            !before_is_word && !after_is_word
        })
        .count() as u32
}

fn is_ascii_word_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn create_supported_files(root: &Path, count: usize) {
        fs::create_dir_all(root).expect("create fixture");
        for index in 0..count {
            fs::write(
                root.join(format!("entry-{index}.ts")),
                format!("export const value{index} = {index};"),
            )
            .expect("write fixture file");
        }
    }

    #[test]
    fn scans_supported_files_and_counts_export_references() {
        let root = std::env::temp_dir().join(format!("octocode-graph-scan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).expect("create fixture");
        fs::write(
            root.join("src/entry.ts"),
            "export const answer = 1; console.log(answer);",
        )
        .expect("write fixture");
        fs::write(root.join("src/large.ts"), "x".repeat(64)).expect("write oversized fixture");
        fs::write(root.join("README.md"), "# ignored").expect("write ignored file");

        let result = scan_graph_facts(GraphFactsScanOptions {
            path: path_string(&root),
            max_files: Some(10),
            max_file_bytes: Some(48),
            ..Default::default()
        })
        .expect("scan graph facts");

        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.candidate_paths, ["src/entry.ts", "src/large.ts"]);
        assert_eq!(result.files_skipped, 1);
        assert_eq!(result.schema_version, 1);
        assert_eq!(result.skipped.len(), 1);
        assert_eq!(result.skipped[0].relative_path, "src/large.ts");
        assert_eq!(result.skipped[0].code, "graph.scan.fileTooLarge");
        assert_eq!(result.entries[0].relative_path, "src/entry.ts");
        assert_eq!(result.entries[0].reference_counts[0].name, "answer");
        assert_eq!(result.entries[0].reference_counts[0].count, 2);
        assert!(!result.truncated);
        fs::remove_dir_all(root).expect("cleanup fixture");
    }

    #[test]
    fn reports_stable_read_and_extraction_failures() {
        let root = std::env::temp_dir().join(format!(
            "octocode-graph-scan-diagnostics-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create fixture");
        fs::write(root.join("invalid.js"), [0xff, 0xfe]).expect("write invalid utf8 fixture");
        let extraction_size = crate::minify::minifier::MAX_SIZE + 1;
        fs::write(root.join("extract.ts"), vec![b'x'; extraction_size])
            .expect("write extraction fixture");

        let result = scan_graph_facts(GraphFactsScanOptions {
            path: path_string(&root),
            max_files: Some(10),
            max_file_bytes: Some(extraction_size as u32),
            ..Default::default()
        })
        .expect("scan graph facts");

        assert_eq!(result.files_skipped as usize, result.skipped.len());
        assert_eq!(
            result
                .skipped
                .iter()
                .map(|diagnostic| (diagnostic.relative_path.as_str(), diagnostic.code.as_str()))
                .collect::<Vec<_>>(),
            [
                ("extract.ts", "graph.scan.extractFailed"),
                ("invalid.js", "graph.scan.readFailed"),
            ]
        );
        fs::remove_dir_all(root).expect("cleanup fixture");
    }

    #[test]
    fn preserves_reference_counts_from_both_fact_producers() {
        let root = std::env::temp_dir().join(format!(
            "octocode-graph-scan-reference-metadata-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create fixture");
        fs::write(
            root.join("main.ts"),
            "export function alpha() { return alpha; }",
        )
        .expect("write TypeScript fixture");
        fs::write(root.join("lib.rs"), "pub fn beta() { let _ = beta; }")
            .expect("write Rust fixture");

        let result = scan_graph_facts(GraphFactsScanOptions {
            path: path_string(&root),
            ..Default::default()
        })
        .expect("scan graph facts");

        assert_eq!(
            result
                .entries
                .iter()
                .map(|entry| (
                    entry.relative_path.as_str(),
                    entry.reference_counts[0].name.as_str(),
                    entry.reference_counts[0].count,
                ))
                .collect::<Vec<_>>(),
            [("lib.rs", "beta", 2), ("main.ts", "alpha", 2)]
        );
        fs::remove_dir_all(root).expect("cleanup fixture");
    }

    #[test]
    fn typed_scan_preserves_structural_ids_ranges_edges_and_content_receipt() {
        let root =
            std::env::temp_dir().join(format!("octocode-graph-typed-scan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create fixture");
        fs::write(
            root.join("lib.rs"),
            "pub fn alpha() { beta(); }\nfn beta() {}",
        )
        .expect("write fixture");

        let result = scan_graph_facts_typed(GraphFactsScanOptions {
            path: path_string(&root),
            ..Default::default()
        })
        .expect("typed scan");
        let entry = result.entries.first().expect("entry");
        assert_eq!(entry.facts.file, "lib.rs");
        assert!(!entry.content_digest.is_empty());
        assert!(entry
            .facts
            .declarations
            .iter()
            .all(|decl| !decl.id.is_empty()));
        assert!(entry
            .facts
            .declarations
            .iter()
            .all(|decl| decl.range.end >= decl.range.start));
        assert!(entry.facts.edges.iter().all(|edge| !edge.id.is_empty()));
        fs::remove_dir_all(root).expect("cleanup fixture");
    }

    #[test]
    fn reports_truncation_only_when_graph_scan_exceeds_the_file_cap() {
        let root =
            std::env::temp_dir().join(format!("octocode-graph-scan-cap-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        create_supported_files(&root, 3);

        let below_cap = scan_graph_facts(GraphFactsScanOptions {
            path: path_string(&root),
            max_files: Some(4),
            ..Default::default()
        })
        .expect("scan below cap");
        assert_eq!(below_cap.entries.len(), 3);
        assert!(!below_cap.truncated);

        let exact_cap = scan_graph_facts(GraphFactsScanOptions {
            path: path_string(&root),
            max_files: Some(3),
            ..Default::default()
        })
        .expect("scan at cap");
        assert_eq!(exact_cap.entries.len(), 3);
        assert!(!exact_cap.truncated);

        let over_cap = scan_graph_facts(GraphFactsScanOptions {
            path: path_string(&root),
            max_files: Some(2),
            ..Default::default()
        })
        .expect("scan over cap");
        assert_eq!(over_cap.entries.len(), 2);
        assert!(over_cap.truncated);

        fs::remove_dir_all(root).expect("cleanup fixture");
    }

    #[test]
    fn detects_graph_scan_overflow_across_directory_boundaries() {
        let root = std::env::temp_dir().join(format!(
            "octocode-graph-scan-nested-cap-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        // Each child ends exactly at the initial cap, whichever is visited first.
        create_supported_files(&root.join("left"), 1);
        create_supported_files(&root.join("right"), 1);

        let capped = scan_graph_facts(GraphFactsScanOptions {
            path: path_string(&root),
            max_files: Some(1),
            ..Default::default()
        })
        .expect("scan nested graph at cap");
        assert_eq!(capped.entries.len(), 1);
        assert!(
            capped.truncated,
            "the sibling subtree still has a graph file"
        );

        let expanded = scan_graph_facts(GraphFactsScanOptions {
            path: path_string(&root),
            max_files: Some(2),
            ..Default::default()
        })
        .expect("expand nested graph scan");
        assert_eq!(
            expanded.candidate_paths,
            ["left/entry-0.ts", "right/entry-0.ts"]
        );
        assert!(
            !expanded.truncated,
            "an exactly full complete scan has no overflow"
        );
        fs::remove_dir_all(root).expect("cleanup fixture");
    }

    #[test]
    fn ignored_files_do_not_make_an_exact_graph_cap_partial() {
        let root = std::env::temp_dir().join(format!(
            "octocode-graph-scan-filtered-cap-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        create_supported_files(&root, 1);
        fs::write(root.join("ignored.txt"), "not graph input").expect("write ignored file");
        fs::create_dir_all(root.join("ignored-directory")).expect("create ignored subtree");
        fs::write(
            root.join("ignored-directory/ignored.txt"),
            "not graph input",
        )
        .expect("write ignored child");

        let result = scan_graph_facts(GraphFactsScanOptions {
            path: path_string(&root),
            max_files: Some(1),
            ..Default::default()
        })
        .expect("scan exactly one matching graph file");
        assert_eq!(result.candidate_paths, ["entry-0.ts"]);
        assert!(
            !result.truncated,
            "nonmatching entries are not graph overflow"
        );
        fs::remove_dir_all(root).expect("cleanup fixture");
    }

    fn path_string(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn policy_denied_graph_files_do_not_consume_the_scan_budget() {
        let root =
            std::env::temp_dir().join(format!("octocode-graph-policy-{}", std::process::id()));
        fs::create_dir_all(root.join("private")).expect("fixture");
        fs::write(root.join("private/hidden.rs"), "pub fn hidden() {}").expect("hidden");
        fs::write(root.join("visible.rs"), "pub fn visible() {}").expect("visible");
        let result = scan_graph_facts_filtered(
            GraphFactsScanOptions {
                path: path_string(&root),
                max_files: Some(1),
                ..Default::default()
            },
            &|path| Ok(!path.starts_with(root.join("private"))),
        )
        .expect("filtered scan");
        assert_eq!(result.candidate_paths, ["visible.rs"]);
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.files_skipped, 0);
        assert!(!result.truncated);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn graph_read_enforces_limit_when_file_grows_after_discovery() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let root =
            std::env::temp_dir().join(format!("octocode-graph-growth-{}", std::process::id()));
        fs::create_dir_all(&root).expect("fixture");
        let file = root.join("growing.rs");
        fs::write(&file, "pub fn initial() {}").expect("initial");
        let visits = AtomicUsize::new(0);
        let result = scan_graph_facts_filtered(
            GraphFactsScanOptions {
                path: path_string(&root),
                max_file_bytes: Some(32),
                ..Default::default()
            },
            &|path| {
                if path == file && visits.fetch_add(1, Ordering::SeqCst) == 1 {
                    fs::write(&file, "x".repeat(65_536)).expect("concurrent growth");
                }
                Ok(true)
            },
        )
        .expect("bounded scan");
        assert!(result.entries.is_empty());
        assert_eq!(result.skipped[0].code, "graph.scan.fileTooLarge");
        fs::remove_dir_all(root).expect("cleanup");
    }
}
