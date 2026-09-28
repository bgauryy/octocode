# Graph research: optional Clasify ablation

Freeze this protocol before running. Primary KPI: total measured host input + output tokens across four paired source-navigation tasks. A promising result requires at least 5% reduction, all eight answers correct with directly observed deciding source, and no increase in tool errors. Also report wall time, calls, Clasify calls and cached input. Provider usage is unknown unless actual receipts expose it; never infer total cost from output size.

Corrected harness, version 3: answers are graded by declared type. Identifier tuples preserve order but permit whitespace around commas; variants permit a bare name or qualification by the correct enum; line numbers are positive decimal integers. The grader binds the full deciding statement to its exact line in a directly observed source result, including numbered text pages when structured rows are absent. Broad range metadata or Clasify judgments alone are insufficient. These rules are frozen before the new eight-trial campaign; earlier campaign artifacts are immutable. Every observed MCP catalog must equal the frozen catalog; repeated identical startup catalogs are valid. V2 consumed one trial before its overly strict single-startup guard stopped the campaign; that campaign remains INCONCLUSIVE and is not pooled with V3. V3 has a separate authorized budget of eight fresh trials. Incomplete or invalid campaigns report no comparative token-reduction number. Credential copies are removed in a `finally` block on success or failure.

Baseline: localSearch, structureSearch, localFetch. Candidate adds optional clasify. Both receive canonical core instructions for their exact available catalog. This evaluates the source-navigation part of graph research; AST/LSP graph correctness is evaluated separately against live tools. It cannot attribute differences solely to provider inference: schema/instruction overhead and the routing decision are part of the treatment.

Four source-derived cases: parsing-cache identity, conditional evidence-graph materialization, snapshot generation identity, and an exact-symbol negative control. One attempt per arm; counterbalanced order; no tuning or retries after results. These are development-selected, public-source cases, **not held-out or statistically representative**. The expected answers and source hashes are outside each isolated fixture. The fixture contains one verbatim Rust source file, with no repository skills or project instructions. Shared task wording specifies the answer format but never asks the solver to use Clasify.

Reuse the existing app-server isolation and fixture-only MCP proxy. Fresh HOME/CODEX_HOME and ephemeral session per trial; disabled inherited tools, skills and project instructions; explicit model and effort; real downstream MCP schemas/runtime; no output schemas exported. Source citations must be backed by localFetch/localSearch evidence. Budget: eight trials, 180 seconds and 12 calls per trial, at most 50 classification cells. Invalid sessions stop the campaign and remain recorded. Do not interpret one paired sample as a production recommendation.

```sh
FLOW_MODEL=gpt-6-astra FLOW_EFFORT=high node packages/octocode-benchmark/compare/graph-research-v1/run.mjs init /absolute/campaign
node packages/octocode-benchmark/compare/graph-research-v1/run.mjs run /absolute/campaign
node packages/octocode-benchmark/compare/graph-research-v1/run.mjs report /absolute/campaign
node packages/octocode-benchmark/compare/graph-research-v1/selftest.mjs
```
