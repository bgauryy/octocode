# Agent workflows on Octocode 20.2.0

Across 17 tools, workflow and result quality average **8.9/10**, implementation **8.9**, agentic use **8.6**, and context **8.5**. The most reliable workflow in this sample was reading exact source from a known path or a pinned GitHub result. The largest opportunity is helping an agent form its first valid classifier or graph query.

## Per-tool ratings

These are editorial ratings for the five requested dimensions. Workflow grades the handoff to the next tool. Context grades the amount and clarity of guidance an agent must read. Implementation grades observed execution, reuse, failure handling and available regression evidence. Quality grades the returned evidence against the sampled reference. Agentic use grades whether an agent can choose, continue, recover and know when it has enough evidence. Scale anchors are 9 for strong evidence with minor friction, 8 for a material but manageable tradeoff, and 7 for recurring friction. The means give every tool equal weight. No model-success probability or release-readiness score was computed.

| ID | Tool | Workflow | Context | Implementation | Quality | Agentic | Deciding observation |
|---|---|---:|---:|---:|---:|---:|---|
| L1 | `structureSearch` | 9 | 9 | 9 | 9 | 9 | Listed both TypeScript files. Each path worked as a read target. |
| L2 | `localSearch` | 9 | 8.5 | 9 | 9 | 9 | A literal hit supplied the exact source line and enclosing declaration. A scalar input also produced a normalization warning. |
| L3 | `localFetch` | 9.5 | 9 | 9 | 9.5 | 9 | The two numbered lines reconstructed the file exactly. The CLI and MCP agreed. |
| I1 | `astSearch` | 9 | 8 | 9 | 9 | 8.5 | Found the exported function. File versus snippet input and parser selection take more context. |
| I2 | `lspSearch` | 9 | 8.5 | 9 | 9 | 8.5 | Reached the right definition and disclosed its syntax fallback. An agent must read the evidence level before claiming semantic coverage. |
| R1 | `ghSearchRepo` | 9 | 9 | 9 | 9 | 9 | The public lookup gave the owner, repository and default branch for the next call. |
| R2 | `ghSearchCode` | 8.5 | 8.5 | 8.5 | 8.5 | 8.5 | The source continuation opened the exact hit at its commit. Historical searches still need the distinction between the index and the requested ref. |
| R3 | `ghStructure` | 9 | 9 | 9 | 9 | 9 | The pinned tree identified the README and its 13 bytes. The file reader returned those bytes. |
| R4 | `ghGetFileContent` | 9.5 | 9 | 9 | 9.5 | 9 | Pinned content matched the GitHub blob through both interfaces. A plain README source follow-up also produced a declaration-outline warning. |
| H1 | `ghSearchHistory` | 9 | 8 | 9 | 9 | 8.5 | Three one-commit pages matched GitHub in order. Operation and revision selection add input complexity. |
| H2 | `ghGetHistoryItem` | 9 | 8 | 9 | 9 | 8.5 | The commit supplied separate executable file and patch continuations. Both led to the same README change. |
| P1 | `artifactSearch` | 8.5 | 9 | 8.5 | 8.5 | 8.5 | The Go registry confirmed the release hash. A source hosted outside GitHub needs a different reader. |
| H3 | `ghCloneRepo` | 9 | 9 | 9 | 9 | 9 | The detached checkout matched the requested SHA. The CLI reused the MCP checkout. Large-repository first-fetch cost is still unmeasured. |
| G1 | `graphBuild` | 9 | 9 | 9 | 9 | 9 | The snapshot named two files, one import and one linked call. The query consumed its identifier directly. |
| G2 | `graphQuery` | 8.5 | 7.5 | 9 | 9 | 8 | Returned the correct dependency and source line through both interfaces. Choosing among 21 operations carries a context cost. |
| S1 | `securityCheck` | 9 | 8.5 | 9 | 9 | 8.5 | The package answer matched OSV and distinguished advisory evidence from a maintenance heuristic. |
| C1 | `clasify` | 8 | 7 | 8 | 8 | 7.5 | Preview exposed the destination, selected evidence and request ceiling. Validation substituted an unrelated question in its example. |

The first twelve rows are default tools. The final five require opt-ins or provider configuration.

## Workflow checks

- **Local source, 9/10.** Directory discovery returned `alpha.ts` and `main.ts`. Text search found the exact call on line 2, AST found the exported function, and definition lookup returned line 1 of `alpha.ts`. The TypeScript server was unavailable in the isolated home. The result explicitly identified its native syntax fallback. The CLI and MCP returned the same two lines from `main.ts`.
- **GitHub source and history, 9/10.** Code search opened the README at its resolved commit. The tree, file reader and checkout agreed with the provider’s 13-byte blob. Paging returned three unique commits in GitHub order. Both file and patch continuations worked. The sample exhausted five distinct emitted continuations across tools.
- **Graph investigation, 8.5/10.** Two TypeScript files produced one import and one linked call. Supplying the build receipt’s graph ID to `deps` returned `main.ts` importing `alpha.ts` on line 1. Both interfaces agreed. Operation choice accounts for most of the deduction.
- **Release provenance and advisory research, 8.5/10.** The Go proxy confirmed the full release hash of `golang.org/x/text@v0.14.0`. The tool explained why it had no GitHub handoff. For a separate npm package, OSV returned zero advisories for the sampled version, which agreed with `securityCheck`. The result identified the maintenance signal as a heuristic.
- **Classification setup and recovery, 7.5/10.** An invalid array of questions elicited a valid example shape but replaced the user’s arithmetic question with a credential-rotation question. Rewriting the original intent in the required keyed-object shape yielded a preview with its destination and request ceiling. A model call was outside this recheck. The preceding staged audit used a deterministic loopback provider for the inference and saved-detail paths.

## Context and evidence

A fresh stdio MCP server published **50,662 bytes** of compact UTF-8 `tools/list` JSON and **2,767 bytes** of instruction text by default, totaling **53,429 bytes**. With all tools enabled the respective figures were **80,499**, **3,518** and **84,017 bytes**. The largest individual definitions were `clasify` at 11,189 bytes, `graphQuery` at 10,526 bytes, and `astSearch` at 7,783 bytes. The first two are opt-ins. These counts cover the published catalog and instruction string. Client wrappers, conversation history and tokenization are outside the measurement.

The pinned macOS arm64 binary reported `20.2.0+8ddcbe96`, SHA-256 `c8d019c43bad23f0511f909dc54b914ecfee3f87873165ee0d2ef87e4f25aefe`. All 17 tools received a public call. The classifier preview used a loopback-only endpoint. CLI/MCP comparisons covered local and remote reads, graph dependency results and checkout reuse. Reference checks used the fixture files, Git, GitHub, the Go proxy and OSV. Confidence is medium for these workflows and lower for general classifier answer quality. This round did not rerun the full server/grammar matrix, a paid classifier, comparative throughput, or a blind model-completion study.

Highest-priority improvements are to retain the user's question in classifier validation examples, narrow the graph operation guidance present at discovery, and make AST file/snippet parser selection easier at first use. Preserve the full operation schemas and executable continuations when reducing upfront context.

[Sampled tool responses and catalog measurements](../../../campaigns/20.2.0-2026-11-22-agent-workflows/receipts.json) · [Previous contract and behavior checks](../20.2.0-2026-11-22-staged-audit/CORE-GUIDANCE.md) · [Repository ratings rubric](../../../RATINGS.md)
