use super::*;

use std::path::Path;

/// Walks `options` and scans what the walk finds, with the time budget
/// `budget` (`None` reads every candidate).
fn scan_with_budget<T: Send>(
    options: GraphFactsScanOptions,
    admit: &(dyn Fn(&Path, Option<fs::FileType>) -> bool + Sync),
    keep: &(dyn Fn(&str) -> bool + Sync),
    map: &(dyn Fn(&GraphFactsView<'_>) -> Option<T> + Sync),
    stop: &Stop<'_>,
    budget: Option<std::time::Duration>,
) -> Result<GraphFactsMappedScan<T>, GraphScanError> {
    let walked = walk_graph_fact_files(&options, admit, stop)?;
    evaluate_walk(options, walked, FactsSource::Graph, keep, map, stop, budget)
}

/// A scan keeping every parsed file whole.
#[derive(Debug)]
struct Scanned {
    entries: Vec<GraphFactsTypedEntry>,
    skipped: Vec<GraphFactsScanDiagnostic>,
    files_skipped: u32,
    truncated: bool,
    scan_resume: Option<u32>,
}

/// Every file the scan read or skipped, sorted.
fn scanned_paths(result: &Scanned) -> Vec<String> {
    let mut paths: Vec<String> = result
        .entries
        .iter()
        .map(|entry| entry.relative_path.clone())
        .chain(result.skipped.iter().map(|skip| skip.relative_path.clone()))
        .collect();
    paths.sort_unstable();
    paths
}

fn scan_admitted(
    options: GraphFactsScanOptions,
    admit: &(dyn Fn(&Path, Option<fs::FileType>) -> bool + Sync),
) -> Result<Scanned, GraphScanError> {
    let scan = scan_with_budget(
        options,
        admit,
        &|_| true,
        &|view| Some(view.to_entry()),
        &|| false,
        Some(crate::scan_budget::SCAN_TIME_BUDGET),
    )?;
    Ok(Scanned {
        entries: scan.kept.into_iter().map(|(_, entry)| entry).collect(),
        skipped: scan.skipped,
        files_skipped: scan.files_skipped,
        truncated: scan.truncated,
        scan_resume: scan.scan_resume,
    })
}

fn scan_graph_facts(options: GraphFactsScanOptions) -> Result<Scanned, GraphScanError> {
    scan_admitted(options, &|_, _| true)
}

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

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

mod budget;
mod caps;
mod extraction;
mod reuse;

mod admission;
