//! Every retained store draws on the one process byte budget
//! ([`octocode_engine::memory::MEMORY`]): no store keeps its own entry count or size
//! limit, and only the budget's owners build a raw memo or a budget.
use std::path::{Path, PathBuf};

/// The files allowed to build a raw `Memo` or a `MemoryBudget`: the budget's
/// owner and the page memo every tool store is built from.
const OWNERS: [&str; 2] = ["cache.rs", "tools/page_memo.rs"];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("source directory") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// Where `code` (one line, comments cut) breaks the one-budget rule.
fn breaks(relative: &str, code: &str, names_store: bool) -> Option<&'static str> {
    for constructor in ["PageMemo::new(", "ScanMemo::new("] {
        if let Some(at) = code.find(constructor)
            && !code[at + constructor.len()..].starts_with(')')
        {
            return Some("a store constructor takes a limit");
        }
    }
    // `Memo::new(` as a whole name: `PageReplayMemo::new(` is no memo.
    let raw_memo = code.match_indices("Memo::new(").any(|(at, _)| {
        !code[..at]
            .chars()
            .next_back()
            .is_some_and(|before| before.is_alphanumeric() || before == '_')
    });
    if (raw_memo || code.contains("MemoryBudget::new(")) && !OWNERS.contains(&relative) {
        return Some("a raw memo or budget outside the budget's owners");
    }
    let declares_limit = code.trim_start().starts_with("const MAX_")
        || code.contains(" const MAX_")
        || code.contains("MemoLimits");
    // The owner declares the one budget's bounds; no store declares its own.
    (names_store && declares_limit && !OWNERS.contains(&relative))
        .then_some("a store file declares its own limit")
}

#[test]
fn no_store_sets_its_own_limit_or_bypasses_the_one_budget() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    let mut offenders = Vec::new();
    for file in files {
        let relative = file
            .strip_prefix(&src)
            .expect("under src")
            .to_string_lossy()
            .replace('\\', "/");
        if relative == "tools/page_memo/budget_tests.rs" {
            continue;
        }
        let text = std::fs::read_to_string(&file).expect("source");
        let names_store = text.contains("PageMemo") || text.contains("ScanMemo");
        for (index, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or_default();
            if let Some(why) = breaks(&relative, code, names_store) {
                offenders.push(format!("{relative}:{}: {why}: {}", index + 1, line.trim()));
            }
        }
    }
    assert!(offenders.is_empty(), "{offenders:#?}");
}

/// The budget follows the host: an eighth of its memory, within the floor
/// and ceiling, and the floor where the host's memory cannot be read.
#[test]
fn the_budget_is_an_eighth_of_host_memory_within_its_bounds() {
    use octocode_engine::memory::{MAX_MEMORY_BUDGET, MIN_MEMORY_BUDGET, memory_budget_for};
    const MIB: u64 = 1024 * 1024;
    assert_eq!(memory_budget_for(None), MIN_MEMORY_BUDGET);
    assert_eq!(memory_budget_for(Some(1024)), MIN_MEMORY_BUDGET);
    assert_eq!(memory_budget_for(Some(u64::MAX / MIB)), MAX_MEMORY_BUDGET);
    let between = (MIN_MEMORY_BUDGET + MAX_MEMORY_BUDGET) / 2;
    let host_mb = u64::try_from(between).expect("fits") * 8 / MIB;
    assert_eq!(memory_budget_for(Some(host_mb)), between);
}

/// A tool store's value counts against the budget the engine's own stores
/// share, and leaves it when evicted.
#[test]
fn a_tool_store_draws_on_the_process_budget() {
    let memo = super::PageMemo::<()>::new();
    let weight = 4096;
    let before = octocode_engine::memory::MEMORY.used();
    memo.put("budget-s".into(), "p".into(), weight, ());
    assert!(octocode_engine::memory::MEMORY.used() >= before + weight);
    let (_, held) = memo.contents();
    assert!(held >= weight, "the key bytes count too: {held}");
    memo.evict("budget-s");
    assert_eq!(memo.contents().1, 0);
}
