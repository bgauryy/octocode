# Shared instruction flow experiment

This campaign compares the core-owned MCP research instructions. The catalog,
schemas, native runtime, tool descriptions and model are identical between arms.
It uses the existing isolated app-server runner and the real MCP server, including
the configured Clasify provider. It never exports output schemas.

Frozen protocol (before trials; harness revision 4):

- Primary KPI: aggregate actual host input + output tokens (cached input included).
  Candidate needs at least 5% fewer on validation, with all answers and citations
  correct in both arms. Report cache counts separately; this is not a billing claim.
- Guards: no prohibited tools/access, no timeout or missing usage, no incorrect or
  unverified source claims; candidate tool errors must not exceed baseline.
- Budget: one candidate, two development pairs and four validation pairs; one run
  per arm/case, 180 seconds, at most 12 calls and 50 Clasify cells per trial.
- Cases: exact lookup and semantic location in development; fresh randomized exact
  lookup, semantic location, late-file evidence with hostile source comments, and
  supplied-state classification in validation. Arm order alternates by case.
- Development receipts may be inspected; candidate is frozen before validation.
  Validation answers are generated at initialization, stored outside solver roots,
  and never sent to the solver. Do not inspect them until both arms finish.
- Each trial uses a fresh ephemeral session and home. The runner disables shell,
  other servers, skills, memory and web; the proxy allows only localSearch,
  structureSearch, localFetch and clasify. Nested Clasify reads obey the same scope.
  Path admission resolves symlinks. No resource server exposes evaluator artifacts.
  Fixtures live in temporary directories outside the repository ancestry; setting
  project_doc_max_bytes to zero alone does not disable project skill discovery.
- Grade exact values and source line citations against generated fixtures, and
  require a direct tool result containing the cited source. Clasify verdicts alone
  cannot satisfy source verification. Supplied-state answers need no source read.
- Verdict is exploratory KEEP/DISCARD/INCONCLUSIVE. Six synthetic cases, one sample
  each, are not evidence of general production improvement. Provider token usage
  is unavailable from the current public result; report unknown, never zero.

Revision 1 development trials were invalidated after a blocked attempt to read a
repository skill. Revision 2 moved fixtures outside project ancestry but still
exposed global skills through HOME; a blocked read invalidated that run too.
Revision 3 isolates both HOME and CODEX_HOME, matching the offline runner selftest.
Both arms rerun; the candidate, grading rules, budgets and threshold are unchanged.

Candidate 1 missed the 5% token target (3.5% reduction). Its valid campaign is
retained with DISCARD; the baseline also appended prose to exact-label answers.
The second and final candidate further compresses shared instructions. Revision 4
makes the exact-value formatting requirement explicit in both arms and generates
fresh instances, including the supplied-state case. Threshold, grading and per-
campaign budget remain unchanged. Prior validation is development evidence now;
fresh instances do not establish generalization beyond these synthetic templates.
The run stores source snapshots as well as hashes for its frozen harness.

Run `node selftest.mjs` first. Initialize with `node run.mjs init <absolute-run-dir>`;
put baseline.json and candidate.json (tools + instructions) there before trials.
Use `node run.mjs dev|validation <absolute-run-dir>` to execute pairs, then
`node run.mjs report <absolute-run-dir>`. Model and effort come from the frozen
manifest, explicitly set at initialization via FLOW_MODEL/FLOW_EFFORT.
