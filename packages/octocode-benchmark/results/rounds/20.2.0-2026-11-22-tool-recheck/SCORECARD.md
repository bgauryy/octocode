# Octocode tool recheck, 22 November 2026

All 17 tools were checked through public interfaces. The repeat pass fixed ref evidence in projected GitHub results, unusable retries, misleading repairs, npm search totals, and the handling of a failed data source in a scout. Five upstream contract changes were verified in an isolated checkout. They still need to land in the sibling project.

## Ratings

Scores follow the [weighted rubric](../../../RATINGS.md). Each overall score is a weighted mean over its measured dimensions and combines the named audit, local regression, and public-service observations. The repository, code, and advisory scores include local fixture results for the fixes. Their final positive provider paths remain unverified. “Weight” is the measured fraction of the full rubric. Every score is provisional. No common cold/warm timing and token comparison was run. The question IDs identify the corresponding evidence below.

| Question and tool | Correctness 30 | Input 15 | Output 15 | Paging 10 | Workflow 10 | Efficiency 15 | Freshness 5 | Score / 10 | Weight | Confidence |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| L1 · `structureSearch` | 9 | 9 | 9 | 9 | 9 | — | — | **9.0** | 80% | Medium |
| L2 · `localSearch` | 9 | 9 | 9 | 9 | 9 | — | — | **9.0** | 80% | Medium |
| L3 · `localFetch` | 9 | 9 | 9 | 9 | 9 | — | — | **9.0** | 80% | Medium |
| I1 · `astSearch` | 9 | 8 | 9 | 9 | 8 | — | 9 | **8.7** | 85% | Medium |
| I2 · `lspSearch` | 9 | 9 | 9 | 9 | 9 | — | 9 | **9.0** | 85% | Medium |
| G1 · `graphBuild` | 9 | 9 | 9 | — | 9 | — | 9 | **9.0** | 75% | Medium |
| G2 · `graphQuery` | 9 | 8 | 9 | 9 | 9 | — | 9 | **8.8** | 85% | Medium |
| C1 · `clasify` | 8 | 8 | 8 | 8 | 8 | — | 8 | **8.0** | 85% | Medium |
| R1 · `ghSearchRepo` | 9 | 9 | 8 | 9 | 8 | — | — | **8.7** | 80% | Medium |
| R2 · `ghSearchCode` | 8 | 8 | 7 | 9 | 8 | — | — | **7.9** | 80% | Medium |
| R3 · `ghStructure` | 9 | 9 | 9 | 9 | 9 | — | — | **9.0** | 80% | Medium |
| R4 · `ghGetFileContent` | 9 | 9 | 9 | 9 | 9 | — | — | **9.0** | 80% | Medium |
| H1 · `ghSearchHistory` | 9 | 8 | 8 | 8 | 8 | — | — | **8.4** | 80% | Medium |
| H2 · `ghGetHistoryItem` | 9 | 6 | 9 | 9 | 8 | — | — | **8.3** | 80% | Medium |
| H3 · `ghCloneRepo` | 8 | 8 | 8 | — | 7 | — | 9 | **7.9** | 75% | Low |
| P1 · `artifactSearch` | 8 | 7 | 8 | 8 | 8 | — | — | **7.8** | 80% | Medium |
| S1 · `securityCheck` | 8 | 9 | 8 | 9 | 8 | — | — | **8.3** | 80% | Medium |

The scores describe the tested working-tree behavior and specify where a live service supplied positive evidence. Repeating three probes on the already-running MCP service still lost requested-ref evidence in `ghSearchCode`, omitted an explicitly requested false `archived` in `ghSearchRepo`, and paired severity and vector from different records in a compact `securityCheck` answer. Each of those live-service results is capped at 4/10 under the correctness rule. Their fixes passed local regressions. The configured service's build fingerprint was unavailable.

## Evidence by question

| Question and tool | Deciding evidence and coverage limit |
|---|---|
| L1 · `structureSearch` | Both directory views matched all 11 fixture paths. The scalar sort repair now suggests an executable field. Large directory and timing coverage is limited. |
| L2 · `localSearch` | Thirty hit lines contained 60 occurrences. Context and line-window recovery were checked separately. Remote and warm-server throughput were outside this round. |
| L3 · `localFetch` | A 34-part Unicode read and a 59-part single-line byte walk reassembled exactly. A match may need a second read for the complete line. |
| I1 · `astSearch` | The refreshed variants require a file path or content with a parser. Both forms returned 301 nodes and continued across CLI/MCP in 15 and 14 pages. The flat selected view still obscures the required choice. |
| I2 · `lspSearch` | Eight TypeScript reference sites matched the language service. Searching for the missing anchor resolved the intended export on line 49. The language sample was TypeScript and Rust. |
| G1 · `graphBuild` | Build, reuse, forced refresh and stale detection matched the fixture. The seven-file graph had seven symbols and five linked calls. Interrupted construction was not retested. |
| G2 · `graphQuery` | All 21 operations were invoked in the audit. Grouped dependency evidence narrowed from two edges to one import. The refreshed operation describes re-export uses. The selected edge field still needs the precise imports-only override. |
| C1 · `clasify` | The connected scout selected one expected repo from eight rows over three billed pages. Focused regressions covered 128 quiet scores, saved detail, cumulative budget and preservation of predicate identity. Positive provider execution on the refreshed host was unavailable. |
| R1 · `ghSearchRepo` | Seven ordered repositories matched the provider in the audit. Sixteen GitHub tests jointly covered repository and code shaping, including an explicit false archived field. Positive verification used the configured service. |
| R2 · `ghSearchCode` | The prior live result dropped requested-ref evidence on three repositories. Local regressions distinguish present, missing and unverified candidates. The older live MCP still drops projection provenance. |
| R3 · `ghStructure` | Twenty directory entries matched the provider. The rebuilt error path preserved a runnable transient retry for two public repos. On the configured service, a materialized README yielded seven hits over two local-search pages and matched the remote read for lines 1–25. |
| R4 · `ghGetFileContent` | Selected source lines and long-line bytes matched the provider. A Vite outline and the selected function body agreed on their span at the same commit. Positive reads used the configured service. |
| H1 · `ghSearchHistory` | Three merged PR identities matched the provider. Narrowing stopped repeating its own bound in the regression. Date and merged-state corrections were checked on rebuilt CLI and MCP. |
| H2 · `ghGetHistoryItem` | Patches, issue bodies, two comments, 16 PR files and 40 reviews matched provider evidence. The pinned description still lists invalid section choices for commits. |
| H3 · `ghCloneRepo` | The public retry reopened the saved request on both interfaces. Five offline clone/reuse checks and one dirty-checkout check passed. A first clone over the public network remains unverified. |
| P1 · `artifactSearch` | All 34 dependencies for eslint 9.0.0 matched its pinned manifest. Fixtures fixed two npm search errors. The pinned single-term description still overstates literal matching. |
| S1 · `securityCheck` | A 16-page live walk preserved source-specific advisories and affected/fixed scope. The repaired compact severity is confirmed in recorded data. A live positive from the final binary remains unverified. |

## Workflows

The twelve workflows below follow the [benchmark checklist](../../../CHECKS.md). “Live” means a successful public call. Time, token use and call-count parity with prior releases were unmeasured.

| Question | Observed workflow |
|---|---|
| Find a GitHub symbol and read its body | A code candidate led to an exact Vite source read. At commit `28032deefa0e0da08c517092aeb418756870e433`, the `definePlugin` block covered lines 7–98. The caller supplied the independently resolved commit when the older projected result omitted it. |
| Locate a folder and read an outline | A pinned tree listed 19 plugin files. Its `define.ts` outline located the same declaration at lines 7–98. |
| Read a file's history and newest selected patch | The commit patch matched the provider after both parts were joined. |
| Read a merged PR, files and reviews | PR discovery matched GitHub. The selected PR detail walk reconstructed its body, 16 files and 40 unique reviews. |
| Compare a tagged release | Two adjacent Vite releases and a compare call placed the selected change among the ten commits added for v2.0.1. |
| Trace a package version to source | npm `vite@2.0.1` resolved to a tag and commit. The pinned file contained both selected `process.env` matches. |
| Assess a package version | Package and purl agreed on the semver advisory. Queries for 7.5.1 and 7.5.2 distinguished the affected version and the fixed boundary. See the [source advisory](https://osv.dev/vulnerability/GHSA-c2qf-rxjj-qqgw). |
| Clone a repository and search repeatedly | The public retry and the offline checkout, reuse and preservation fixtures passed. A live public clone followed by three local searches remains unverified. |
| Find a local symbol and read it | Local search and read fixtures preserved individual hits, enclosing declarations and long-line bytes through pagination. |
| Resolve local callers | Local search found `reviewedTarget` on line 1. The TypeScript-backed callers query found `reviewedCaller` and its call on line 2, column 43, matching the fixture. |
| Find unreachable local code | A graph built over explicit entrypoints identified unreachable files and cited callers. Re-export use was distinguishable from a direct import. |
| Scout unread sources and verify evidence | File, URL and section scouts led to the public README text `Hello World!`. A repository predicate identified `octocat/Hello-World`. A fresh exact lookup returned its repository metadata. |

## Security and data boundaries

| Tool class | Data and action | Checked boundary |
|---|---|---|
| Local search, fetch, structure, AST and LSP | Read workspace paths and source. LSP starts language processes for supported workspaces. | Planted local credential files remained withheld in the sampled discovery views. Stale AST and LSP cursors returned restarts. Warm sockets and broad language coverage were unverified. |
| Graph build and query | Persist a workspace-derived snapshot and traverse its evidence. | A query continued across interfaces with the same snapshot. Changes produced drift evidence. |
| GitHub reads and clone | Send repository coordinates, queries and refs to GitHub. Clone writes a local checkout. | Ref verification and transient retry behavior have regressions. Offline fixtures preserved local edits on a dirty checkout. |
| Package and advisory lookup | Send package coordinates, repository or commit selectors to the configured providers. | Package and repository scope stayed distinct. A non-registry URL reported that nothing was checked. Records retained provider-specific severity evidence. |
| Classifier and scout | Send selected evidence to the configured inference provider. Fresh evaluation and newly assessed pages can bill. | Pages of 3, 3 and 2 units used one provider request apiece, three in total. Saved section replay was marked historical. Billed continuation and source transport failure were checked separately. Raw network egress capture was unavailable. |

## Build and upstream state

The baseline native audit pinned `20.2.0+61ba9e9e` at SHA-256 `6cf335773c732d146100d7a56ae4e5c0792a5d69ab84d824badd15a2c8b8199a`. The fix verification pinned `20.2.0+064df0a9` at SHA-256 `dd7b9fc4f43cd55eff211c26020b679beb5853e3e333aae8a47cb39e0fb9f8a2`. The configured service could reach external providers. The local process could exercise fixture and failure paths, and could not establish a positive connection to public registries or GitHub.

On the pinned fix build, all 17 CLI and fresh MCP tools had matching descriptions, resolved query schemas and output field references. For each tool, the two interfaces returned matching structured errors for its tested invalid input. CLI returned exit 2 and MCP marked these responses as errors. A clipped UTF-8 search hit led to a full 31,916-byte read on both interfaces. An unavailable GitHub source stayed a retryable transport error in `clasify` and its emitted source call ran unchanged.

After a concurrent regeneration, the rebuilt host reported `20.2.0+c53cf52d` at SHA-256 `1b9b2df88043c4f2d0de6372bcd4d7037e7538b1b5fd46752ff3cd696399fd24`. All 17 tools passed the same two-interface checks again. The refreshed contract includes separate AST file and snippet variants, file/package dependent re-export guidance, and classifier walk accounting. The AST suite passed 418 tests. The graph suite passed 905 tests in the preceding pass.

Five upstream changes remain in a separate patch for selected-schema AST guidance, history sections by operation, graph edge defaults in the selected view, npm keyword matching, and how predicate scores appear and page. The combined patch applies cleanly to the sibling working tree. In an isolated copy, all 1,879 core tests across 138 files passed. The typecheck, build and generated-presentation checks passed. The default, classification-enabled and full MCP descriptions each stayed within their existing size limits. The combined patch SHA-256 is `6314ad2e06e26060261fbc43f49671da83f7ebb5e47c85f30cb4d0a4ff7c1981`. The sibling checkout was outside the writable roots. The refreshed binary does not include that patch.

The connected service charged one request per page on the three-page scout. The refreshed contract and runtime carry the request allowance across a saved source walk and expose `usage.walkCalls`. A fixture starting with four prior requests and a cap of five refused a two-request page without spending and accepted an allowance of six. Independent fresh calls still require a caller-managed total. A positive provider-backed walk on the refreshed host remains unverified.

The final native build was pinned at SHA-256 `62f112ed08c3e87f01e814f2f45fafa22a261bea4809a1afa2195ad1b08949a7`. A shared-request regression verified that the two rows kept their respective walk totals and cursor debits of 1 and 3 even when batch billing attributed the calls as 2 and 0. Eight source-progress tests and the pricing regression passed. A fresh MCP server matched the preceding verified build on all 17 discovery contracts and all 17 invalid-call results. On both CLI and MCP, a public request over two local files with a budget of one returned `classificationOverBudget`, `usage.calls: 0` and an executable confirmation cursor in both summary and detail views. A temporary diagnostic that could write full classifier results was removed from the shared source. Neither the diagnostic file path nor the temporary assertion markers was found in the final binary.

## Repository checks

The full Rust library test run recorded 8,040 passes and 1,275 failures. Five failures were outdated graph receipt or AST redaction expectations. All five subsequently passed focused reruns. The other 1,270 failures traced to denied sockets or a filesystem permission bit that the host stripped. A separate process-test run passed 442 tests and failed 50 socket-dependent tests. The configuration suite passed 176 tests and failed one loopback test. Documentation and contract consistency checks passed.

The repository's static check, including Clippy and TypeScript, passed after the final cleanup. The original aggregate run was stopped after a skill test waited indefinitely for a denied loopback listener. The repaired fixture reports the bind error immediately. The complete skill-script rerun finished with 207 passes and 21 failures, including denied listeners and unavailable debugger or proxy behavior. The release gate remains red in this environment.
