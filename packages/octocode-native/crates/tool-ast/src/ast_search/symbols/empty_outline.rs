use octocode_contract_types::AstSearchQuerySymbols;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::policy::discovery::WalkSkips;
use octocode_tools_shared::leads::skipped;
use serde_json::Value;

/// An empty directory outline cannot prove absence in what its walk skipped:
/// the scan honors `.gitignore` and the default prune, and hides dot entries.
/// The hint names the paths the walk skipped (only those that could hold an
/// outlinable file), and `includeSkipped` (as `match` and the text tools
/// offer it) is the same scope with exactly their causes lifted. Nothing
/// behind them is read, as ripgrep leaves `-u` to its user.
pub(super) fn note_skipped(q: &AstSearchQuerySymbols, walked: &WalkSkips, out: &mut Value) {
    if out["status"] != "empty" {
        return;
    }
    let Some(disclosure) = skipped::unsearched_ledger(walked) else {
        return;
    };
    let lead = skipped::lead(ToolId::AstSearch, q, &disclosure.causes);
    skipped::attach(out, &disclosure, lead);
    // The response keeps the first leads of `next`: the text fallback goes
    // after the leads that lift what the outline skipped.
    if let Some(next) = out["calls"].as_object_mut()
        && let Some(fallback) = next.shift_remove("findText")
    {
        next.insert("findText".to_owned(), fallback);
    }
}
