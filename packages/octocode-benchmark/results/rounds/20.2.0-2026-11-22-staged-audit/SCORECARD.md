# Staged tool and core audit

**17 tools checked, 8.6/10 provisional mean.** The three previously capped connected results passed their regressions. A new contradictory Go release warning caps `artifactSearch` at 4.0. The public clone and three-search workflow passed on both frozen builds.

The follow-up rerates all 17 tools and core guidance after the package warning, viewer export, classifier wording, graph guide and AST schema fixes. [Updated ratings and verification](CORE-GUIDANCE.md)

## Tool ratings

These are readiness judgments on a ten-point scale. They combine fresh checks with the [preceding scorecard](../20.2.0-2026-11-22-core-context/SCORECARD.md) and apply the [correctness cap](../../../RATINGS.md). The mean gives each tool equal weight. This round did not repeat the full twelve-dimension benchmark, comparative timings, all language servers, or every operation on the staged build. Confidence is medium for the sampled behaviors. Classifier inference used a deterministic loopback provider.

| Tool | Previous | Current | Deciding evidence |
|---|---:|---:|---|
| `structureSearch` | 9.0 | 9.0 | Four TypeScript paths across two pages agreed with the filesystem. |
| `localSearch` | 9.0 | 9.0 | All 180 matching lines arrived exactly once across 14 pages on the first pin. The staged three-page search also matched source. |
| `localFetch` | 9.0 | 9.0 | Six pages reconstructed 37,150 UTF-8 bytes on the first pin. The staged Unicode fixture also reconstructed exactly. |
| `astSearch` | 8.8 | 8.8 | The structural query found the real call. The qualified Rust follow-up found both implementations. The selected snippet schema repeats the parser requirement. |
| `lspSearch` | 9.0 | 9.0 | Five references over five pages agreed with the TypeScript service, including two columns on a re-export line. |
| `graphBuild` | 9.0 | 9.0 | Four files, two imports and one linked call matched source. A repeat reused the snapshot and adding a file exposed staleness. |
| `graphQuery` | 8.4 | 8.6 | Both re-export dependents arrived over two pages. An explicit imports-only selector returned one, and the unreachable-file query identified the planted orphan. |
| `clasify` | 8.4 | 8.4 | The provider counted zero calls for an over-budget two-file scout. One request produced 128 scores. Saved details returned all 128 without another provider request. The description sends the reader to the wrong field. |
| `ghSearchRepo` | 8.8 | 8.8 | Explicit archive and default-branch fields survived projection for two active repositories. |
| `ghSearchCode` | 8.7 | 8.7 | Two projected paths absent at historical refs retained the requested commit and `atRef: false`. Independent trees confirmed both absences. |
| `ghStructure` | 9.0 | 9.0 | Four pages returned all 12 root entries with the Git tree's types and sizes. |
| `ghGetFileContent` | 9.0 | 9.0 | All eight selected file lines agreed with the tagged Git object. |
| `ghSearchHistory` | 8.6 | 8.6 | Four pages returned seven commits and subjects in GitHub's order. |
| `ghGetHistoryItem` | 8.3 | 8.8 | GitHub confirmed the patch with two additions and four deletions. The description, selected commit schema and validation repair now agree on valid sections. |
| `ghCloneRepo` | 8.0 | 9.0 | A fresh public clone resolved the annotated tag to the verified commit and three local searches found lines 6, 7 and 1. Clean reuse, sparse checkout and preservation of a dirty file passed on the first pin. |
| `artifactSearch` | 8.4 | **4.0** | All eight ecosystems resolved a version. All 34 eslint dependencies matched npm. On two Go modules the warning contradicted the correct immutable ref in the same answer. |
| `securityCheck` | 9.0 | 9.0 | The compact CVE result retained high severity and omitted the unrelated vector. The four-page detail walk preserved the distinct source records. Affected and fixed braces versions agreed with OSV. |

The GitHub checks used `sindresorhus/is-plain-obj` and `sindresorhus/is-absolute-url`. Both native pins and the connected MCP passed the archive and requested-ref probes. The connected MCP also passed the advisory regression. Its executable fingerprint was unavailable. [Cloned source at the verified commit](https://github.com/sindresorhus/is-plain-obj/tree/8fd127e76e8a4532116aeff73e52ff3a7bb8fa25) · [Independent patch](https://github.com/sindresorhus/is-plain-obj/commit/69581e4e218f70d689009712b2b5b1e0f12834e7) · [OSV advisory](https://osv.dev/vulnerability/CVE-2022-25883)

## Open issues

| ID | Owner and impact | Reproduction and acceptance |
|---|---|---|
| A1 | Native package lookup tells the reader to discard usable release provenance. | Resolve `golang.org/x/text@v0.14.0` and `golang.org/x/net@v0.21.0`. The rows return refs beginning `6c97a165` and `73d21fdb` that agree with the Go proxy. Both warn that no release ref is given. The warning should acknowledge the returned ref and explain that there is no GitHub handoff. [Warning source](../../../../octocode-native/crates/tool-packages/src/artifact_search/leads.rs) |
| A2 | The graph viewer stops before exporting a completed native graph. The pinning guide gives the same incorrect field name. | Build a small graph and call the viewer adapter against the staged CLI. The receipt contains `data.graph`, while the adapter tests `data.id` and throws. Export should complete and an explicit query should consume the published snapshot identifier. [Adapter](../../../../../skills/octocode-graph-view/scripts/graph-pages.mjs) · [Guide](../../../../../skills/octocode-graph/references/build-and-read.md) |
| A3 | Core discovery directs agents to `units` for classifier search hits. | Scout 16 matching lines with eight predicates, then run `next.details`. Both replies put the 128 scores in `files[path][line]`. The description should distinguish search hits from other units, as the output contract already does. Sibling anchors are `packages/octocode-core/src/toolContract/tools/clasify.ts:295` and `src/toolContract/outputClasify.ts:147`. |
| A4 | The graph guide's checkout fallback fails in the default zsh shell. | Run the documented scalar assignment and subsequent `$OCTO graphBuild` invocation. zsh exits 127, treating the runtime and script path as one executable name. The checkout instructions should run verbatim in zsh. [Graph skill](../../../../../skills/octocode-graph/SKILL.md) |
| A5 | The selected AST schema repeats a parser requirement. | Run `octocode schema astSearch --select operation=syntaxTree --json`. The content field repeats “needs parser.” once. The file and snippet inputs themselves work. |

## Core, binary and release evidence

**Core contract quality: 8/10**, a qualitative assessment of the checked contract and guidance. The copied sibling sources at `219c5f2` plus working changes passed **1,882 tests in 140 files**, typecheck, build and public-catalog generation. The source-generated canonical enforcement contract matched the checkout by SHA-256. The agent-context checks passed 26 tests. The native inventory mapped all 17 tools with no unclassified input fields. The config suite passed **177 tests in 23 files** and the config generator's consistency check passed. The AST, graph, history, npm and classifier areas from the older five-change patch were checked against current behavior. The exact historical patch was not used as evidence of what shipped.

| Frozen native binary | Checks |
|---|---|
| `20.2.0+4e45f303` · SHA-256 `cbb55be57e213e680ac0bda52f86763f9755a8b2f5ba3194d2b33ad55a2aa73a` | Direct native and fresh MCP probes, provider oracles, page walks and regression checks. |
| Staged macOS arm64 `20.2.0+04e4fd9e` · SHA-256 `881395dd1e695e7f26ddd38c798a8ce1036c508544a22f0b0d0e1de82e815813` | Representative positive calls for every tool, plus the new Go warning, classifier replay, graph viewer failure and the public clone with three searches. All 17 CLI and fresh MCP descriptions, resolved query schemas, output references and sampled invalid responses matched. The staged binary passed the host freshness check. |

The initial package freshness failure cleared when the other session staged the second binary. Main checkout HEAD was `2460a496f` with concurrent working changes. The full native release gate requires a stable shared tree and was not run in this audit. The earlier sandbox gate result belongs to its earlier candidate. The classifier source and printable binary strings were checked for the reported diagnostic patterns. The original pathname was unavailable for an exact exclusion. No paid inference was used.
