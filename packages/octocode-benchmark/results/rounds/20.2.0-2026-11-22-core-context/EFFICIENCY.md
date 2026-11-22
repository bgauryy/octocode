# Workflow receipts, 22 November 2026

One direct pass covered all [12 workflows](../../../CHECKS.md#workflows-each-must-finish-in-the-same-number-of-calls-as-last-round). Ten completed against the selected scope. A bounded scout verified one line from a pinned source. The public clone stopped before any local searches.

| Workflow | Surface | Calls (failed) | Input B | Output B | Call time (ms) | Inference requests | Scope and verification |
|---|---|---:|---:|---:|---:|---:|---|
| Search a symbol, read its body | MCP | 2 (0) | 340 | 6,319 | 9,957 | — | Vite `definePlugin`, lines 12–168 at `3567471998e3da315675086f44368e266c9c0e33`, matched the GitHub source. |
| Folder, then an outline | MCP | 2 (0) | 328 | 716 | 201 | — | Vite `v2.0.1` plugins folder returned 19 files. The selected `define.ts` outline span, lines 7–98, matched GitHub. |
| A file's history, then the newest patch | MCP | 2 (0) | 355 | 1,182 | 784 | — | `octocat/Hello-World` `README`, 14–15 September 2011. Commit `762941318ee16e59dabbacb1b4049eec22f0d303` and its one-file patch matched GitHub. |
| A merged PR, then its files and reviews | MCP | 2 (0) | 322 | 843 | 1,052 | — | Vite PR `#19107`, one changed file and two reviews. The filenames and review records matched GitHub. |
| A tag, then one file's change between releases | MCP | 2 (0) | 352 | 1,064 | 1,018 | — | Vite `v2.0.0` → `v2.0.1`, `packages/vite/package.json`. GitHub confirmed the comparison and both sides of the version patch. |
| A package version, then its source at that release | MCP | 2 (0) | 247 | 413 | 1,592 | — | `semver@7.5.2` resolved to `e7b78de06eb14a7fa2075cedf9f167040d8d31af`. Line 3 of the pinned manifest confirmed the root version. |
| Is this version affected | MCP | 1 (0) | 96 | 234 | 1,526 | — | `semver@7.5.1` affected, `7.5.2` fix. The OSV range confirmed the verdict. [Advisory](https://osv.dev/vulnerability/GHSA-c2qf-rxjj-qqgw) |
| Many searches in one repository | CLI | 2 (2) | 204 | 873 | 5,071 | — | Sparse clone of `octocat/Hello-World` at the README commit above. Both the initial call and its returned retry failed with a transport error. Three local searches remained blocked. |
| Local find, then read | CLI | 2 (0) | 177 | 881 | 96 | — | One declaration hit in a disposable TypeScript project. The `reviewedTarget` body matched fixture lines 2–4. |
| Local find, then who calls it | CLI | 2 (0) | 175 | 828 | 699 | — | TypeScript LSP returned `reviewedCaller` and its only call site at line 6, column 10, matching the fixture. |
| Local find, then what is unused | CLI | 2 (0) | 170 | 1,530 | 259 | — | Three TypeScript files with an explicit `main.ts` entry point. The graph reported only the planted orphan `app/unused.ts`. |
| Scout unread evidence, then read proof | MCP | 2 (0) | 410 | 469 | 1,188 | 1 | `Hello-World` README line 1 at the pinned commit. The returned `next.read` matched GitHub. Large-source coverage remains open. |

MCP rows used the connected Octocode service, whose binary fingerprint was unavailable. CLI rows used a frozen working-tree build of `20.2.0+4e45f303` at checkout `097919873`, SHA-256 `f01e0af6730d376d62ee963d06fad2d54e5b283d5f513305de81910aacad94da`. This pass has a separate build identity from the [scorecard](SCORECARD.md).

Input counts compact JSON arguments. Output counts text payloads for MCP and both output streams for CLI. Call time sums the duration of each invocation, excluding between-call waits, setup and verification. The scout reported 377 input and 20 output provider tokens under `maxRequests: 1`. A dash denotes a workflow with no inference step. Dollar cost, release latency and parity with prior call counts remain unmeasured. Verification checked the original GitHub source, the OSV advisory or the planted local fixture, and consumed the required continuations within the selected scope.

## Same-input timing sample

Each first/repeat pair below used identical inputs and is separate from the receipts above. The remote rows used the same connected MCP service with uncontrolled shared caches. Their first timings mean first observed in the pair. Each local row started a fresh MCP process on the pinned CLI build with a fresh Octocode home and fixture, disabled LSP prewarm, and repeated the calls inside that process. Timings exclude MCP initialization. The sandbox refused the local cache server socket, so these pairs ran in-process. Host filesystem caches were uncontrolled.

| Workflow | Timing surface | First call sum (ms) | Repeat call sum (ms) |
|---|---|---:|---:|
| Symbol and body | Connected MCP | 9,926 | 232 |
| Folder and outline | Connected MCP | 200 | 189 |
| File history and patch | Connected MCP | 474 | 199 |
| Merged PR files and reviews | Connected MCP | 1,074 | 573 |
| Release comparison | Connected MCP | 579 | 236 |
| Package and pinned manifest | Connected MCP | 3,349 | 484 |
| Version affected | Connected MCP | 1,585 | 103 |
| Clone and searches | CLI | Unmeasured | Unmeasured |
| Local find and read | Local MCP | 73 | 10 |
| Local callers | Local MCP | 744 | 22 |
| Local unused file | Local MCP | 220 | 26 |
| Scout and proof | Connected MCP | Unmeasured | Unmeasured |

Both local attempts matched fixture evidence. The same graph snapshot was returned and queried in both passes. Connected rows for symbol/body, PR files/reviews and version affected returned different serialized data in the timing pair. Evidence verification for those rows comes from the first table. Clone was already blocked and the scout retained its single inference request. Their only observed timings appear above.
