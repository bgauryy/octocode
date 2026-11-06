//! Corpus sweep for the one-pass nesting bound (`signatures/js_oxc/nesting/
//! scan.rs`). For every JS/TS file under the given roots that the pass
//! vouches for, the bound must be at least the depth of its syntax tree,
//! whether or not the tree holds an `ERROR` or `MISSING` node. It prints each
//! file where it is not, the largest depth - bound and its file for clean and
//! for error trees, and how many files the pass vouched for (the rest pay for
//! a syntax tree before oxc reads them). Run:
//! `cargo run --profile profiling -p octocode-engine --example nesting_sweep -- <root>...`

use std::path::{Path, PathBuf};

const SKIPPED_DIRS: &[&str] = &["target", "node_modules", "dist", "out", "build", "coverage"];

fn walk(root: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if !path.is_dir() {
            found.push(path);
        } else if !name.starts_with('.') && !SKIPPED_DIRS.contains(&name.as_str()) {
            walk(&path, found);
        }
    }
}

/// The largest depth - bound seen, and its file.
#[derive(Default)]
struct Worst(Option<(isize, String)>);

impl Worst {
    fn see(&mut self, gap: isize, file: &str) {
        if self.0.as_ref().is_none_or(|(largest, _)| gap > *largest) {
            self.0 = Some((gap, file.to_owned()));
        }
    }

    fn print(&self, trees: &str) {
        if let Some((gap, file)) = &self.0 {
            println!("largest depth - bound over {trees} trees {gap} in {file}");
        }
    }
}

fn main() {
    let mut files = Vec::new();
    for root in std::env::args().skip(1) {
        walk(Path::new(&root), &mut files);
    }
    let (mut checked, mut vouched, mut errors, mut unsure_bytes, mut below) = (0, 0, 0, 0, 0);
    let (mut clean_worst, mut error_worst, mut oxc_worst) =
        (Worst::default(), Worst::default(), Worst::default());
    for path in &files {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        let name = path.to_string_lossy();
        let Some(read) = octocode_engine::signatures::nesting_sweep_check(&source, &name) else {
            continue;
        };
        let (depth, has_error) = (read.tree_depth, read.tree_error);
        checked += 1;
        let Some(bound) = read.bound else {
            unsure_bytes += source.len();
            println!("UNSURE {name} ({} bytes)", source.len());
            continue;
        };
        vouched += 1;
        let gap = depth.cast_signed() - bound.cast_signed();
        if has_error {
            errors += 1;
            error_worst.see(gap, &name);
        } else {
            clean_worst.see(gap, &name);
        }
        if let Some(oxc) = read.oxc_depth {
            let oxc_gap = oxc.cast_signed() - bound.cast_signed();
            oxc_worst.see(oxc_gap, &name);
            if oxc_gap > 0 {
                println!("OXC-BELOW {name}: bound {bound} < oxc depth {oxc}");
            }
        }
        if gap > 0 {
            below += 1;
            println!("BELOW {name}: bound {bound} < depth {depth} (error tree: {has_error})");
        }
    }
    clean_worst.print("clean");
    error_worst.print("error");
    oxc_worst.print("oxc AST");
    println!(
        "checked {checked} vouched {vouched} (error trees {errors}) unsure {} ({unsure_bytes} bytes) below {below}",
        checked - vouched
    );
    if below > 0 {
        std::process::exit(1);
    }
}
