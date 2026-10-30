Load when choosing which layer answers a question, reading a verdict, or a layer disagrees. Why: each layer answers a different question and fails differently; a verdict needs agreement, never one layer alone.

# Evidence layers

Each layer answers a different question and fails differently; a verdict needs agreement, never one layer alone. The `--layers` ids are ast, clasify, ocr, history and lsp (graph always runs); usage runs with every verify, image review needs clasify, and DOM layout comes from render.

| Layer | Answers | Fails when |
|---|---|---|
| Graph (`octocode graph`) | Cycles, hubs, upward imports, ranked findings | Syntax-only linkage: calls are leads, not proof |
| AST (`astSearch`) | Is every graph edge a parsed import statement or an inline qualified path (expression or type); a target is matched by its file stem or, for a Rust crate root, its Cargo package name | Languages outside the kind table fall back to text |
| Usage | Are the imported names used after the import (also in Rust child modules that inherit imports with `use super::*`), and call-like | Misses method calls through a value of an imported type, macros, re-exports; a `conflict` verdict is the cue to read the file |
| Clasify | Named facts about a file: type-only vs runtime, unrelated owners; one named cohesion follow-up when the first answer is unsure | Generic questions score near 0.65 for any file; name things from the graph |
| Image review | Crop around each lead edge: is it drawn, are both labels readable | Dense areas: treat as a doubt flag |
| DOM layout | Label overlaps, labels on nodes, clipping (measured) | Never guesses; Functions view is crowded by design |
| OCR (`glm-ocr` as a FreeLlama-managed vision task, one affinity session per run; raw Ollama only without the sidecar) | Whole-image label read-back; missed labels retried on a 3x crop | Loops once the visible text ends: token caps per strip and per label, retries at half and a quarter of the budget, then crops; the sidecar refuses under host memory pressure (503 with `Retry-After`) |
| History (git) | Churn, hotspots (fan-in x commits), co-change from small commits, hidden coupling | Squashed history: commits touching more than 40 files are ignored for co-change; GitHub history is empty when the remote lacks the path |
| LSP (opt-in) | Resolved call hierarchy and references: `verify --layers lsp` for a lead symbol, `build --calls both` for every probed function (edges labelled graph, lsp or both) | Cold start 20 s to 2 min, then 0.1-1 s per call; call hierarchy skips calls inside closures; partial analysis (`buildScriptsOff`, file errors) returns nothing, so coverage can be low |
