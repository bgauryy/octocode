# Platform lane — live validation (2026-09-30)

Scope: clasify, config layering/trust, OCTOCODE_BETA gating, skills, install, auth, error envelopes/exit codes, MCP-vs-CLI parity, bulk goal/reasoning rules.
Raw outputs: `features/probes/platform/` (scripts: `cfg/run-layers.sh`, `cfg/run-trust.sh`, `mcp-probe.mjs`, `clasify-mcp.mjs`, `mcp-beta-rc.mjs`; clasify requests/results in `clasify/`).
Paid clasify provider calls: 9 (CLI 5, MCP 4 non-cached; plus 2 cached repeats).
Environment note: from ~00:38 to 00:53 another agent's native rebuild caused contract-fingerprint drift (MCP init failed, CLI `scheme` exit 5). All MCP/clasify probes below ran after the rebuild finished.

## Counts

VERIFIED 39 · PARTIAL 14 · FAILS 5 (all doc-vs-behavior) · UNTESTABLE 0 (58 rows)

## Feature table

| # | Feature | Documented | Probe | Status | Evidence |
|---|---|---|---|---|---|
| 1 | clasify Judge (context.value) | docs/OCTOCODE_CLASIFY.md:5-6,288-320 | `octocode clasify --input clasify/judge.json` | VERIFIED | exit 0; `support:{choice:"contradicted",probabilities:{contradicted:.46,insufficient:.41,supported:.13}}`, `started:{noul:.69}` |
| 2 | clasify Scout `locate` | OCTOCODE_CLASIFY.md:136 | `clasify --input clasify/locate.json` (CONFIGURATION.md + SECURITY.md, fullContent) | VERIFIED | best.prec[0] CONFIGURATION.md 215-222 exists .99; source 217-222 is the precedence block |
| 3 | preset `sufficient` | skills/octocode-research/references/clasify.md (Questions); schema questionType enum | same matrix, `enough` question | VERIFIED | `enough:{noul:.97}` (CONFIGURATION.md) vs `.40` (SECURITY.md) |
| 4 | Matrix batching questions×resources | OCTOCODE_CLASIFY.md:118-121,334-344 | locate.json (2×2); clasify/cells-30.json; too-many-cells.json | VERIFIED | 4 cells in one call; 6×5 → "Expanded resources × questions produces 30 cells; maximum is 25." exit 2; 26 questions → "Value length 26 exceeds the maximum of 25" |
| 5 | Scout over list (per-file pages + next.read) | OCTOCODE_CLASIFY.md:189-198 | `clasify --input clasify/scout-list.json` (localSearch OCTOCODE_BETA, contribution+sufficient) | PARTIAL | 5 pages each with next.read; 3/5 pages `coverage:"partial"` + "incomplete evidence without a safe continuation" (clipped match values); next.read `CONFIGURATION.md 293-413` past EOF (375 lines) — still executes |
| 6 | carry across next.clasify | OCTOCODE_CLASIFY.md:136,345 | `node clasify-mcp.mjs` (locate over OCTOCODE_TOOLS.md, follow next.clasify) | VERIFIED | page1 top exists .05 → no `best`, `carry.ttl` 3 windows; page2 `best.ttl[0]` 1532-1539 exists .98 (Materialization TTL row) + carried 444-451 merged |
| 7 | IDs preserved / derived | OCTOCODE_CLASIFY.md:100-111 | root `queries[]` with 2 matrices via MCP | VERIFIED | `queryId:"mA"`,`held1`,`isStartupBlock` kept; omitted → `matrix-2`,`resource-1`,`question-1` |
| 8 | Judgment cache | OCTOCODE_CLASIFY.md:185 ("No judgment cache exists") | repeat identical call in same MCP process | FAILS (doc) | repeat 149-171 ms vs 950-977 ms, byte-identical output; code: runtime/src/tools/clasify/cache.rs (process-local, SHA-256 key of endpoint+model+state+questions, 256 entries/8 MiB/30-min TTL), used at runtime/clasify_batch.rs:1393; no output indicator; CLI gets no cross-call cache (new process) |
| 9 | No-key: MCP unregisters clasify | OCTOCODE_CLASIFY.md:79; CONFIGURATION.md:215 | `mcp-probe.mjs` env `OCTOCODE_CLASSIFICATION_API=''` (and both blank) | VERIFIED | 12 tools, no clasify; instructions 3596 chars (clasify guidance removed) vs 3926 |
| 10 | No-key: CLI error | OCTOCODE_CLASIFY.md:80 | `OCTOCODE_HOME=<scratch> OCTOCODE_CLASSIFICATION_API= OCTOCODE_JEV_KEY= octocode clasify --input judge.json [--json-errors]` | PARTIAL | exit 5, `{"error":"clasify requires OCTOCODE_CLASSIFICATION_API …","errorCode":"missingConfiguration"}` — actionable, but bare shape (no `kind:"octocode.toolError"`), `--json-errors` has no effect |
| 11 | Blank env disables despite file keys | CONFIGURATION.md:192; OCTOCODE_CLASIFY.md:84 | `OCTOCODE_CLASSIFICATION_API= octocode scheme` (real home has both keys) | VERIFIED | `availability:{enabled:false,…,"hint":"OCTOCODE_CLASSIFICATION_API in a .env file is shadowed by the process environment"}` |
| 12 | next.clasify handoff from wide localSearch | OCTOCODE_CLASIFY.md:185 | localSearch `config` in docs (15 files) → run `data.next.clasify` unchanged | PARTIAL | handoff present and runs (exit 0; best CONFIGURATION.md 231-238 exists .97 = misconfig section), but 3 resources, not "five top-ranked files"; handoff adds `prefilter:["config"]` (undocumented there) |
| 13 | Handoff hidden while clasify disabled | OCTOCODE_CLASIFY.md:185 | same search with `OCTOCODE_CLASSIFICATION_API=` | VERIFIED | no `"clasify"` in next |
| 14 | Leak guard (no bodies) | OCTOCODE_CLASIFY.md:140 | grep all clasify outputs for body strings | VERIFIED | 0 hits for "Materialization TTL"/"OCTOCODE_CACHE_TTL_MS" across c1-c9 + MCP log |
| 15 | MCP clasify text channel | OCTOCODE_CLASIFY.md:140 ("empty text content") | MCP walk | FAILS (doc) | `content[].text` = 2024 bytes of JSON (duplicate of structured payload) |
| 16 | locate + incompatible resource rejected pre-retrieval | OCTOCODE_CLASIFY.md:138 | clasify/locate-incompatible.json (structureSearch) | VERIFIED | exit 2, `classificationLocateUnsupported`, "Matrix rejected before context retrieval or classification." |
| 17 | Duplicate IDs / preset+instructions rejected | OCTOCODE_CLASIFY.md:111,134 | dup-ids.json; preset-mixed.json | VERIFIED | "resources.1.id: Duplicate resources id: a"; "Unknown field(s): instructions" exit 2 |
| 18 | Doc JSON examples runnable | OCTOCODE_CLASIFY.md:245-265 (fileChunks), 290-318 (Judge) | doc-filechunks-example.json, doc-judge-example.json | FAILS | exit 2: "goal: Missing required field", "questions.0.question: Unknown field: question" |
| 19 | clasify CLI exit 6 on continuation | OCTOCODE_CLASIFY.md:366 | `clasify --input clasify/walk.json` | VERIFIED | exit 6, coverage partial, page scope 1-217 of 1566 |
| 20 | Config 5-layer precedence | CONFIGURATION.md:219-230 | `cfg/run-layers.sh` (DISABLE_TOOLS / tools.disabled, distinct tool per layer, observe `scheme`) | VERIFIED | L1 env→ghSearchHistory; L2 ws .env→ghGetFileContent; L3 global .env→ghStructure; L4 ws rc→ghSearchCode; L5 global rc→ghSearchRepo |
| 21 | Blank ws .env value falls back | CONFIGURATION.md:187 | L6 (`DISABLE_TOOLS=` in ws .env) | VERIFIED | global .env ghStructure disabled |
| 22 | Workspace .env trust (dotenv:"home") | docs/generated/CONFIG_SETTINGS.md:72-91; config-contract.json:96 | `cfg/run-trust.sh` T3 (ws .env OCTOCODE_BETA=true), T11 (ws .env GITHUB_API_URL) | PARTIAL | ignored (astTopology stays disabled; `config --json` → `skippedProtected:[{key:"OCTOCODE_BETA",source_path:…/ws/.octocode/.env}]`) but no stderr warning and `diagnostics:[]` |
| 23 | Workspace rc protected field warning | CONFIGURATION.md:211 | ws rc `{"local":{"beta":true}}`, `{"storage":{"mode":"persistent"}}` | PARTIAL | tool call + `config`: "local.beta is protected and ignored in a workspace config file … [workspace_config_protected]"; `scheme` prints nothing |
| 24 | Misconfig never blocks + stderr warnings | CONFIGURATION.md:234-252 | ws rc bad value/unknown key, invalid JSON; home .env REQUEST_TIMEOUT=abc | PARTIAL | exit 0 always; tool/config stderr: "network.maxRetries: Must be a number; value ignored [invalid_config]", "Failed to parse config file … [config_load_error]", "REQUEST_TIMEOUT is not a valid integer … [invalid_env_value]"; `octocode scheme` emits none |
| 25 | `config --json` diagnostics | CONFIGURATION.md:252 | T7 | VERIFIED | `diagnostics:[{code:"unknown_or_future_config",…local.enableLocl},{code:"invalid_config",field_path:"network.maxRetries"}]` |
| 26 | Protected keys blocked in .env | CONFIGURATION.md:302-316 | home .env `PATH=/evil`,`NODE_OPTIONS=--inspect` | VERIFIED | `skippedProtected:[NODE_OPTIONS,PATH]` |
| 27 | storage.mode home-trusted | CONFIGURATION.md:318 | T6 global rc memory + ws rc persistent | VERIFIED | `storage: memory` + `[workspace_config_protected]` warning |
| 28 | GitHub tokens accepted from .env | CONFIGURATION.md:189 | home .env / ws .env `GH_TOKEN=<bogus>` → ghSearchRepo | VERIFIED | exit 4 both (token used, not gh fallback) — contradicts CONFIGURATION.md:360 |
| 29 | OCTOCODE_BETA env → CLI scheme | CONFIG_SETTINGS.md:75 | T0/T1 | VERIFIED | astTopology/astRewrite false → true |
| 30 | local.beta global rc / global .env | CONFIG_SETTINGS.md:75 | T2, T4 | VERIFIED | both enable; `OCTOCODE_BETA=false` env beats rc true (disabled) |
| 31 | MCP beta registration | README.md:208-218; CONFIGURATION.md:358 | `mcp-probe.mjs` beta; `mcp-beta-rc.mjs` (scratch home rc local.beta) | VERIFIED | 14 tools with astTopology, never astRewrite/ghCloneRepo; rc path also registers astTopology |
| 32 | Gated CLI call explains gate | README.md:218 | `env -u OCTOCODE_BETA octocode astTopology/astRewrite '<json>'` | PARTIAL | "astTopology is a beta feature, disabled by default. Set OCTOCODE_BETA=true or local.beta:true…", exit 5, bare `{error,errorCode}` shape, `--json-errors` no effect |
| 33 | ghCloneRepo gate message | README.md:212,218 | `OCTOCODE_STORAGE_MODE=memory octocode ghCloneRepo` | PARTIAL | exit 5 "Tool ghCloneRepo is not available in this native runtime" — does not name storage.mode |
| 34 | skill list/info/check/help | README.md:334,480-485 | `octocode skill list|info octocode-clasify|check|help` | VERIFIED | 16 bundled listed; info prints SKILL.md; check exit 1 ("0/16 ok; 14 stale") |
| 35 | skill install to scratch | packages/octocode/docs/OCTOCODE_CLI.md:393-425 | `skill install octocode-roast --path <scratch> [--dry-run] --mode copy` | VERIFIED | "1 materialized"; SKILL.md, README.md, references/ present |
| 36 | skill remove dry-run scope | OCTOCODE_CLI.md:411 | `HOME=<scratch> OCTOCODE_HOME=<scratch> skill remove octocode-roast --dry-run` | PARTIAL | preview "removed: /Users/bgaryy/code/octocode/.agents/skills/octocode-roast" — acts on cwd project links, not the scratch home |
| 37 | skill remove path traversal | memory roast-fixes (skill-remove traversal) | `skill remove ../../etc --dry-run` | PARTIAL | refused ("0 removed; 1 failed", exit 1) but reason blank ("failed: ") |
| 38 | Skills inventory vs README | README.md:476 | `ls skills`, `skill list` | FAILS (doc) | 16 skills (incl. octocode-agents-communication); README says 15, table omits agents-communication |
| 39 | skills/README links | skills/README.md:14 | ls | FAILS (doc) | links `octocode-architecture-view/` which lives in skills-beta/ |
| 40 | install --help/--list targets | OCTOCODE_CLI.md:341-348 | `octocode install --list` | VERIFIED | 15 targets = doc list (cursor, claude-desktop, claude-code, windsurf, trae, antigravity, vscode-cline/roo/continue, zed, opencode, gemini-cli, kiro, codex, goose) |
| 41 | install --dry-run / --check | CONFIGURATION.md:277 | `HOME=<scratch> install --ide cursor|claude|vscode …` | VERIFIED | writes nothing; `<scratch>/.cursor/mcp.json` preview; `claude`→claude-desktop, `vscode`→vscode-cline; bogus → exit 2; --check on missing → exit 1 |
| 42 | auth status read-only | CONFIGURATION.md:115-121 | `octocode auth --json` | VERIFIED | `tokenSource:"octocode-storage"`, username bgauryy |
| 43 | gh CLI fallback | CONFIGURATION.md:104 | `OCTOCODE_HOME=<scratch> auth --json` (also PATH without gh) | VERIFIED | `tokenSource:"gh-cli"`; found via well-known dirs (providers/github/auth/discovery.rs:10) even off PATH |
| 44 | Token alias priority | CONFIGURATION.md:96-104 | bogus/real pairs → ghSearchRepo | VERIFIED | OCTOCODE_TOKEN(bogus)>GH_TOKEN: 401; GH_TOKEN(real)>GITHUB_TOKEN(bogus): ok; GITHUB_TOKEN(bogus)>GITHUB_PAT: 401; process GITHUB_TOKEN beats .env GH_TOKEN |
| 45 | auth status with invalid token | CONFIGURATION.md:120 | `GITHUB_TOKEN=ghp_bogus… auth status --json` | PARTIAL | `authenticated:true, username:null, publicGitHubAccess:"authenticated"`; `tokenSource:"env"` names neither alias nor .env file |
| 46 | Auth failure exit code | OCTOCODE_CLI.md:563 | bogus OCTOCODE_TOKEN ghSearchRepo | VERIFIED | exit 4, row `errorCode:"authentication",httpStatus:401` |
| 47 | Invalid JSON | OCTOCODE_CLI.md:555,561 | `localSearch '{not json' [--json-errors]` | VERIFIED | exit 2; stderr text / stdout `{"kind":"octocode.toolError","version":1,"error":"Invalid JSON query…","tool":"localSearch"}` |
| 48 | Unknown tool | same | `octocode fooTool '{}'` ; MCP `fooTool` | VERIFIED | CLI exit 2 clap error / toolError envelope; MCP JSON-RPC -32602 "Tool fooTool not found" |
| 49 | Missing goal/reasoning | docs/OCTOCODE_TOOLS.md:59 | CLI + MCP localSearch without goal | PARTIAL | CLI exit 2 actionable ("a new query states it; only a next.* continuation with followUp: true inherits it"); MCP isError with Zod text "expected nonoptional, received undefined" |
| 50 | >5 queries | OCTOCODE_CLASIFY.md:336 / schema | 6 rows | VERIFIED | CLI "Value length 6 exceeds the maximum of 5" exit 2; MCP "Too big: expected array to have <=5 items" |
| 51 | Unknown field | OCTOCODE_TOOLS.md:59 | `bogusField:1` | VERIFIED | CLI "Unknown field(s): bogusField" exit 2; MCP lists valid fields |
| 52 | Out-of-range rejected, not clamped | schema | contextLines 9999; startLine -5 | VERIFIED | "Number is outside the allowed range (0-100)" / "(1-1000000000)", exit 2; MCP "Too big: <=100" |
| 53 | Not found / empty / partial / mixed | OCTOCODE_CLI.md:558-574 | localFetch missing file; no-match search; chunkSize 5; ok+missing batch | VERIFIED | exit 3 row `status:"error",errorCode:"fileAccessFailed"`; exit 1 `status:"empty"`; exit 6 with next.continue; mixed → exit 0 (documented) |
| 54 | MCP tool lists | README.md:208-218 | `mcp-probe.mjs` default | VERIFIED | 13 tools (clasify + 12), no ghCloneRepo/astRewrite/astTopology; CLI scheme toolCount 16 |
| 55 | MCP instructions vs scheme instructions | OCTOCODE_MCP.md | diff `mcp-instructions.txt` vs `scheme.txt.instructions` | VERIFIED | only diff: CLI adds "ghCloneRepo bridges remote evidence to local tools;" (availability-scoped) |
| 56 | Same query both surfaces | TOOL_DATA_CONTRACT.md | localSearch OCTOCODE_TRUST_PROJECT_LSP_CONFIG, with/without debug | VERIFIED | CLI stdout == MCP structuredContent byte-for-byte (803 B / 1049 B) |
| 57 | debug:true meta | OCTOCODE_TOOLS.md:94 | same, debug:true | PARTIAL | adds `meta.evidence{kind:"lexical"}` + `diagnostics{codes:["continuationMissing"],partial:true}` on a complete 2-hit result (only a clipped match value); exit 0 |
| 58 | Per-row goal/reasoning; followUp on hand-written query | OCTOCODE_TOOLS.md:59; README.md:328-329 | row1 without goal; top-level goal; `followUp:true` without goal | PARTIAL | per-row required (row1 → row `invalidInput`, exit 2, row0 runs); top-level goal ignored (still "Missing required field"); hand-written `followUp:true` with no goal/reasoning accepted exit 0 on CLI and MCP |

## Doc drift

- docs/OCTOCODE_CLASIFY.md:185 "No judgment cache exists: every call re-reads and re-judges" → a process-local judgment cache exists (tools/clasify/cache.rs; runtime/clasify_batch.rs:1393). In MCP, repeating the identical call returned byte-identical output in 149 ms vs 950 ms.
- docs/OCTOCODE_CLASIFY.md:185 "one locate matrix over its five top-ranked files" → the handoff carried 3 resources for a 15-file page, each with `prefilter:[searchText]`.
- docs/OCTOCODE_CLASIFY.md:140 "MCP returns a single structured payload with empty text content" → the MCP text channel holds about 2 KB of JSON.
- docs/OCTOCODE_CLASIFY.md:245-265 and 290-318: the examples use a nested `"question":{…}` wrapper and omit the required `goal`. Both are rejected with exit 2.
- docs/OCTOCODE_CLASIFY.md:134: the research presets list leaves out `sufficient`, which is in the schema enum and the skill.
- docs/OCTOCODE_TOOLS.md:59 "Queries batched in one call share one goal and one reasoning" → each row must have its own goal and reasoning. A top-level goal is ignored. The MCP instructions also say "each row with its own goal and reasoning".
- README.md:328-329 "Other tools accept `reasoning` as optional context" → reasoning is required on every tool (exit 2 without it).
- README.md:476 "15 public skills" → there are 16 in `skills/` and `skill list`. The README table leaves out octocode-agents-communication.
- skills/README.md:14 links `octocode-architecture-view/` → that skill is in `skills-beta/`, so the link is broken.
- docs/CONFIGURATION.md:189 "All declared product configuration keys … are accepted from either source" → dotenv:"home" keys (OCTOCODE_BETA, GITHUB_API_URL, ALLOWED_PATHS, WORKSPACE_ROOT, OCTOCODE_STORAGE_MODE, LSP overrides …) are silently skipped from a workspace `.env`.
- docs/CONFIGURATION.md:360 (Troubleshooting) "Token vars are blocked in .env" → tokens are dotenv:"all" and are used from both home and workspace `.env` (exit 4 with a bogus file token). This also contradicts :189.
- docs/CONFIGURATION.md:236 "Each problem is printed once per process to stderr" and :211 "ignored … with a warning" → `octocode scheme` prints no config warnings, and a workspace-.env home-only key is never warned about. Troubleshooting (:340) sends users to `scheme` first.
- docs/CONFIGURATION.md:232 and :253: the same bullet ("GitHub/classification credentials follow this fallback order…") appears twice.
- docs/CONFIGURATION.md:277 `--ide claude` → resolves to claude-desktop, not claude-code, and `vscode` → vscode-cline. Neither alias is in `install --list`.

## Defects

1. **Medium: hand-written `followUp:true` bypasses the goal/reasoning requirement.** Repro: `octocode localSearch '{"queries":[{"path":"<repo>/docs","searchText":"OCTOCODE_BETA","followUp":true}]}'` → exit 0 (MCP behaves the same). The error text says only a `next.*` continuation may inherit goal and reasoning, but nothing ties `followUp` to a real continuation.
2. **Medium: `auth status` reports an invalid token as authenticated.** Repro: `OCTOCODE_HOME=<scratch> GITHUB_TOKEN=ghp_bogus… octocode auth status --json` → `authenticated:true, username:null, publicGitHubAccess:"authenticated"`, while tool calls with that token fail 401/exit 4. `tokenSource:"env"` also fails to say which alias won or whether the token came from a `.env` file, so the documented priority can't be checked from status.
3. **Medium: `skill remove` under an isolated HOME/OCTOCODE_HOME targets the cwd project's `.agents/skills` links.** Repro: from the repo root, `HOME=<scratch> OCTOCODE_HOME=<scratch>/.octocode octocode skill remove octocode-roast --dry-run` → "removed: /Users/bgaryy/code/octocode/.agents/skills/octocode-roast". A non-dry run would delete a repo-local link the scratch install never created.
4. **Low: config/gate errors use a third output shape.** Missing clasify key, beta-gated astTopology/astRewrite, and ghCloneRepo in memory mode all return exit 5 with a bare `{"error","errorCode"}`. That is neither the `octocode.toolError` envelope nor a `results[]` row, and `--json-errors` has no effect. A config-missing error arguably shouldn't be exit 5 ("execution error").
5. **Low: `octocode scheme` suppresses every config warning.** This covers invalid rc, unknown keys, and the workspace-protected `local.beta`, even though those settings change the availability that `scheme` reports. Separately, a home-only key in a workspace `.env` gets no warning on any command (only `config --json` `skippedProtected`).
6. **Low: clipped match values mark results partial with no continuation.** debug `meta.diagnostics` shows `partial:true, continuationMissing` on a complete localSearch result where one match value was clipped. The same condition marks clasify Scout pages `coverage:"partial"` ("incomplete evidence without a safe continuation"). This matches the known matchContentLength F3 item.
7. **Low: clasify Scout `next.read` ranges run past EOF.** Example: `CONFIGURATION.md 293-413` for a 375-line file. The read still executes.
8. **Low: ghCloneRepo in `storage.mode=memory` gives an unhelpful message.** It says "Tool ghCloneRepo is not available in this native runtime" and does not name the storage gate.
9. **Low: `skill remove ../../etc` gives no reason.** It is refused correctly but prints an empty reason ("failed: ").
10. **Low: MCP validation messages are less actionable than the CLI's.** Missing goal on MCP gives Zod "expected nonoptional, received undefined". The CLI instead explains the followUp/continuation rule.
11. **Low (test infra): the harness hangs on init failure.** `octocode-local-testing/harness/mcp-client.mjs` waits the full 240 s when the server exits at init (contract drift) because it has no child-exit handler.
12. **Info: CLI allowed roots are derived from cwd.** Running from a subdirectory rejects absolute paths in parent dirs (`pathOutsideAllowedRoots`, exit 5). This is by design, but it makes CLI-vs-MCP parity depend on cwd.
