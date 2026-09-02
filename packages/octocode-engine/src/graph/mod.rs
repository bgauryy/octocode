use std::fs;

use rayon::prelude::*;

use crate::types::{
    FileSystemQueryOptions, GraphFactsScanEntry, GraphFactsScanOptions, GraphFactsScanResult,
    GraphReferenceCount,
};

const DEFAULT_MAX_FILES: u32 = 20_000;
const DEFAULT_MAX_FILE_BYTES: u32 = 1_000_000;

pub(crate) fn scan_graph_facts(
    options: GraphFactsScanOptions,
) -> Result<GraphFactsScanResult, String> {
    let max_files = options.max_files.unwrap_or(DEFAULT_MAX_FILES);
    let max_file_bytes = options.max_file_bytes.unwrap_or(DEFAULT_MAX_FILE_BYTES) as i64;
    let query = crate::fs_query::query_file_system_inner(FileSystemQueryOptions {
        path: options.path,
        recursive: Some(true),
        show_hidden: Some(false),
        entry_type: Some("f".to_owned()),
        extensions: Some(crate::signatures::graph_facts::graph_fact_extensions()),
        exclude_dir: options.exclude_dir,
        stop_at_limit: Some(true),
        limit: Some(max_files),
        ..Default::default()
    })?;

    let truncated = query.entries.len() >= max_files as usize;
    let mut candidate_paths: Vec<String> = query
        .entries
        .iter()
        .map(|entry| entry.relative_path.replace('\\', "/"))
        .collect();
    candidate_paths.sort_unstable();
    let outcomes: Vec<Option<GraphFactsScanEntry>> = query
        .entries
        .into_par_iter()
        .map(|entry| {
            if entry.size.unwrap_or_default() > max_file_bytes {
                return None;
            }
            let Ok(content) = fs::read_to_string(&entry.path) else {
                return None;
            };
            let relative_path = entry.relative_path.replace('\\', "/");
            let facts_json =
                crate::signatures::extract_graph_facts_inner(&content, &relative_path)?;
            let reference_counts = exported_reference_counts(&content, &facts_json);
            Some(GraphFactsScanEntry {
                relative_path,
                facts_json,
                reference_counts,
            })
        })
        .collect();
    let files_skipped = outcomes.iter().filter(|outcome| outcome.is_none()).count() as u32;
    let entries = outcomes.into_iter().flatten().collect();

    Ok(GraphFactsScanResult {
        entries,
        candidate_paths,
        files_skipped,
        truncated,
    })
}

fn exported_reference_counts(content: &str, facts_json: &str) -> Vec<GraphReferenceCount> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(facts_json) else {
        return Vec::new();
    };
    let Some(declarations) = value.get("declarations").and_then(|value| value.as_array()) else {
        return Vec::new();
    };

    let mut counts = Vec::new();
    for declaration in declarations {
        if declaration
            .get("exported")
            .and_then(|value| value.as_bool())
            != Some(true)
        {
            continue;
        }
        let Some(name) = declaration.get("name").and_then(|value| value.as_str()) else {
            continue;
        };
        let count = count_ascii_word_occurrences(content, name);
        counts.push(GraphReferenceCount {
            name: name.to_owned(),
            count,
        });
    }
    counts
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
        assert_eq!(result.entries[0].relative_path, "src/entry.ts");
        assert_eq!(result.entries[0].reference_counts[0].name, "answer");
        assert_eq!(result.entries[0].reference_counts[0].count, 2);
        assert!(!result.truncated);
        fs::remove_dir_all(root).expect("cleanup fixture");
    }

    fn path_string(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }
}
