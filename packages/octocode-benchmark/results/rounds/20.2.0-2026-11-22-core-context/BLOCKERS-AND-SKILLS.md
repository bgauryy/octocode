# Octocode blockers and skill ratings

22 November 2026 · 17 native tools · 20 public skills · 5 development skills

A blocker here means a supported workflow stops, silently loses requested evidence, gives a materially wrong answer, or incurs unapproved cost. The tool inventory uses the published public-result checks plus targeted replays of earlier correctness failures. The connected GitHub installation did not expose a build fingerprint. Local replays identify the build in the evidence below.

## Confirmed workflow blockers

| Affected skill and user | Reproduction and impact | Fix and acceptance check |
|---|---|---|
| `octocode-graph-view` export and `octocode-graph` explicit snapshot pinning | On a two-file project, native `graphBuild` returned the snapshot under `data.graph`. The viewer requires `data.id` and exits before writing `brief.txt` or `data.json`. Both the installed CLI and verified `20.2.0+b755586b` reproduced it. The graph guide also sends readers looking for the absent `id` when pinning a query. Pinning the actual `graph` value works. | Consume `data.graph` throughout the viewer and name that field in the pinning guide. Test a headless export and a pinned caller query against a real native reply. [Viewer adapter](../../../../../skills/octocode-graph-view/scripts/graph-pages.mjs) · [Pinning guide](../../../../../skills/octocode-graph/references/build-and-read.md) |
| `octocode-graph` checkout fallback and `octocode-dev` audit commands in default zsh | The checkout guides put the runtime and script path into a scalar `$OCTO` or `$DEV`. Executing the scalar exits 127. Some development references also use `$OCTO` after the skill sets up only `octo` and `dev` functions. Direct invocation and the documented functions both passed. | Use the functions consistently or spell out the command and arguments. Run the checkout graph build, dependent query and a development reference command verbatim in a clean zsh session. [Graph guide](../../../../../skills/octocode-graph/SKILL.md) · [Development guide](../../../../../skills-dev/octocode-dev/docs/DEVELOPMENT.md) |

## Native tool coverage

The table accounts for each tool once. The current workflow blockers above affect consumers of native graph results. Tool coverage combines this audit's replays with the [all-tool results](SCORECARD.md) and [workflow receipts](EFFICIENCY.md). “No blocker reproduced” applies to the cases described.

| Tools | Evidence used for the blocker decision |
|---|---|
| `structureSearch`, `localFetch` | The previous result walk returned all five path rows and reassembled the exact 67,221-byte file. |
| `localSearch`, `ghSearchCode` | Both rejected the retired inline classification field during validation. The historical operation that incurred unapproved inference charges could not run through either search. The earlier positive checks recovered the complete local hit and verified five GitHub path rows at requested tags. |
| `astSearch` | On verified build `b755586b`, the historical Rust source produced the expected three `expect`, five `clone` and four `unwrap_or_default` matches. Both known `hide_key_line` hits survived. |
| `lspSearch` | Cross-checked 51/51 TypeScript references for one symbol and 14/14 for another, including built-output and dynamic-import uses. The Rust fixture disclosed the three configuration-excluded files. |
| `graphBuild`, `graphQuery` | Native build returned the two-file snapshot, and the dependent query found its import. The verified binary returned the same 96 direct impact rows at both tested depths in the 3,511-file scope, including the macro use and excluding the sibling-symbol false positive. The shared-tree snapshot indicated staleness. A separate stable fixture passed the same predicates. |
| `clasify` | The previous one-request scout led to the pinned source line. A fresh two-file preflight with a one-request ceiling sent zero requests and returned a confirmation. No paid inference was needed for this audit. |
| `ghSearchRepo`, `ghStructure`, `ghGetFileContent` | The connected service returned both active and archived states. The recorded tree walk reconstructed 20 entries. Current block reads returned the complete sampled Laravel and Symfony methods, including their braces. |
| `ghSearchHistory`, `ghGetHistoryItem` | Recorded checks matched the two history commits, the release compare and a three-page PR patch. The connected recheck returned all 22 provider-omitted Kubernetes patch paths exactly once across 12 pages, with six pinned file reads on page 12. On ripgrep, all four leads from the later page ran to exhaustion across 233 source pages and covered all 12 omitted files at their applicable base or head. The full 411-file Kubernetes query and its six source bodies were outside this replay. |
| `ghCloneRepo` | Public checkout and downstream searches remain an open verification item below. |
| `artifactSearch`, `securityCheck` | Package-to-source handoffs reached immutable releases. A current Go result distinguished a module advisory from the subpackage view and linked to the affected module and its fixed version. The earlier recorded replay retained source-specific advisory vectors. |

## Release verification

| Scope | Observed result | What would close it |
|---|---|---|
| `ghCloneRepo` and its three-search handoff | The opt-in native build returned a transient transport error on a fresh public checkout. The connected installation did not advertise clone. Validation, configuration and an executable retry were checked. | Complete a fresh public checkout with the opt-in build on a network-enabled host, then run the three searches against that checkout. |
| Release gate | `skills:check` inspected all 25 definitions with zero errors and one length warning. Its script suite passed 208 of 228 tests. Local HTTP, WebSocket or debugger tests accounted for the other 20. A separate `node --inspect` invocation confirmed the host denied the inspector listener. | Run the release gate on a host that permits loopback listeners and the Node inspector, and record the exit status for the candidate build. |

## Skill ratings

The scores assess the workflow as written and the artifacts or commands exercised. Each rating is the mean of steps, tool use, output and references, rounded to one decimal on a five-point scale. Five means the reviewed material or tested path met the criterion, four means usable with a limitation, three means partial, two means a major obstacle, and one means the checked path failed. A failed primary workflow remains a blocker regardless of the mean.

**Activation and comparative cost are unmeasured for all 25 skills.** Measuring them requires installed-host runs of own, neighbouring and mixed requests with a matched baseline. Static validation, fixture tests and manual walkthroughs provide different evidence. The scores below are provisional assessments, not six-dimension host benchmark results. [Skill benchmark](../../../SKILLS.md) · [Public request set](../../../questions/skills/README.md)

### Public skills

| Skill | Steps | Tools | Output | References | Rating / 5 | Basis |
|---|---:|---:|---:|---:|---:|---|
| `octocode-agentic` | 5 | 4 | 5 | 5 | **4.8** | The guide reproduces the first wrong decision and replays its guardrails. Tool selection is in a conditional reference. |
| `octocode-brainstorming` | 5 | 4 | 4 | 5 | **4.5** | The guide tests alternatives against original evidence and a counterexample. HTTP-backed research fixtures could not start their local listener. |
| `octocode-chrome-devtools` | 4 | 4 | 4 | 4 | **4.0** | The packaged CLI returned the schema for a navigation command. The guide defines postconditions and evidence. Browser interaction was outside the trial. |
| `octocode-clean-agentic-code` | 5 | 5 | 5 | 5 | **5.0** | The documented workflow defines a before/after oracle and checks public and dynamic consumers. The eight acceptance cases were reviewed on paper. |
| `octocode-debugging` | 5 | 5 | 4 | 5 | **4.8** | The guide preserves the failure, chooses probes that distinguish causes, and repeats the incident. Its saved handoff allows variation. The live-inspector fixture was host-blocked. |
| `octocode-documentation` | 5 | 4 | 5 | 5 | **4.8** | Audience, destination and technical claims each have a verification step. The guide distinguishes commands exercised from commands checked against source. |
| `octocode-engineer` | 5 | 4 | 5 | 5 | **4.8** | The guide compares the current design and alternatives against workload, invariants and failure cases. Branch references supply the operational steps. |
| `octocode-get-started` | 5 | 4 | 5 | 4 | **4.5** | Installer and client subcommand help matched the guide. A local read worked. Installation into a new editor was outside the trial. |
| `octocode-graph` | 3 | 4 | 4 | 3 | **3.5** | Native build and query found the planted import. The zsh checkout command and the guide’s explicit snapshot pin both failed as documented. |
| `octocode-graph-view` | 2 | 2 | 2 | 4 | **2.5** | Preflight and native build succeeded. Headless export stopped before the promised brief and dataset on two native builds. |
| `octocode-messages-proxy` | 5 | 5 | 4 | 5 | **4.8** | Fixture tests passed for routing, ambiguity, timeout, dry-run and vendor errors. A live external relay was outside the trial. |
| `octocode-orchestrator` | 5 | 4 | 5 | 5 | **4.8** | The guide requires one owner, evidence for acceptance and reconciliation after interruptions. It describes how to continue when delegation is unavailable. |
| `octocode-pr-review` | 4 | 5 | 5 | 5 | **4.8** | The workflow pins the PR head and ties the verdict to evidence. Its file-only and unopened-branch routes leave the PR-only trigger ambiguous. |
| `octocode-research` | 5 | 5 | 5 | 5 | **5.0** | A two-file search followed by a one-line fetch recovered the planted source. The guide covers conflicting evidence and unchecked scope. |
| `octocode-rfc-generator` | 5 | 4 | 5 | 5 | **4.8** | A fixture RFC rendered to its sibling HTML file and the manifest query found its topic. The written contract covers alternatives, acceptance and rollback. |
| `octocode-roast` | 5 | 4 | 5 | 5 | **4.8** | The guide pins the target, checks an innocent explanation and traces impact before ranking a finding. |
| `octocode-scraping` | 4 | 4 | 4 | 5 | **4.3** | Local CLI help verified checkpoint, resume and corpus options. Offline corpus tests passed. Live fetch behavior was outside this trial. |
| `octocode-skills` | 5 | 5 | 5 | 5 | **5.0** | The packaged reviewer and self-test ran. The published guidance distinguishes structure, request activation and standalone packaging. |
| `octocode-test-plan` | 5 | 5 | 5 | 5 | **5.0** | The guide requires a named executor, runnable commands and independent pass criteria for critical cases. The grade covers the handoff contract. |
| `octocode-workflows` | 5 | 4 | 5 | 5 | **4.8** | The guide defines dependency order, side-effect reconciliation and a single ledger for execution or resumption. |

### Development skills

| Skill | Steps | Tools | Output | References | Rating / 5 | Basis |
|---|---:|---:|---:|---:|---:|---|
| `octocode-context-audit` | 5 | 5 | 5 | 4 | **4.8** | Sixteen fixture tests covered long pagination, repeated cursors, incomplete evidence, redaction and saved HTML. |
| `octocode-dev` | 4 | 4 | 4 | 3 | **3.8** | Runner help and eight discovery/workspace fixtures passed. The skill’s functions worked, but linked checkout audit commands failed under the documented setup. |
| `octocode-graph-dev` | 3 | 4 | 4 | 4 | **3.8** | The native graph returned the two-file caller and reused its snapshot. The renderer passed 72 checks. Test invocation and receipt locations differ between its guides. |
| `rust-best-practices` | 4 | 4 | 4 | 3 | **3.8** | Compiler version and task-runner slot lookup matched. One repository reference describes crates already present as planned. |
| `zod` | 4 | 4 | 4 | 4 | **4.0** | Nine runtime assertions on Zod 4.6.7 covered object modes, transforms and asynchronous rejection. The integration handoff was reviewed on paper. |
