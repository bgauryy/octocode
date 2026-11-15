# Local CLI tool ratings — live usage

**All 17 research tools produced a positive live result through the requested local CLI.**
No confirmed correctness defect was established in these selected tasks. These are provisional
usage ratings, not release certification or a claim that every operation passes.

## Scope and method

- Launcher: `/Users/bgaryy/code/octocode/packages/octocode/out/octocode.js`.
- Version: **20.2.0**. Native executable: `packages/octocode-native/npm/darwin-arm64/octocode`.
- Launcher SHA-256: `f1f2b6b29a04bca3f9812cc6378dc9cb1a0e7dad2a4126155498d56fe568f8fc`.
- Native SHA-256: `c4f7ee4e8b49188d2a74d3c5d8a274e01c12cad64677a18a72b48104a18ff1c0`;
  checked before and after the live calls, unchanged.
- Round label uses the session date 2026-11-15; the machine reported UTC 2026-11-14 during calls.
- Ratings come from actual calls, answers, errors and workflows, **not implementation review**.
  Help/schema was used to formulate requests; source content was used only as tool input or an oracle.
- Independent controls: authored fixtures, rg/fd, git, GitHub API, npm, PyPI and OSV.
- Only the **CLI** was evaluated. No MCP parity, rebuild, repository implementation edits,
  full test suite, benchmark harness, installation or publication was performed.
- All scores are out of 10, provisional, with medium confidence in the selected positive paths.
  A high score on a small fixture does not establish reliability on a whole monorepo.

## Ratings

Measured weight is the fraction of the repository rubric supported by this round. Unmeasured
is not failure, and is not counted as inapplicable. No efficiency dimension is scored: there
was no matched lean-alternative timing/output experiment. See [dimension data](ratings.json).

| Tool | /10 | Measured | What worked; main limitation |
|---|---:|---:|---|
| localSearch | 8.0 | 55% | File/content hits agree with rg; honest absent-token result. Search paging untested. |
| structureSearch | 8.1 | 80% | 14 files reconstructed in 3+3+3+3+2 pages; fd agrees. Small TS scope only. |
| localFetch | 8.8 | 55% | Exact numbered ranges; clone and classifier read leads work. Large-file paging untested. |
| astSearch | 9.0 | 55% | Exact double(21) call at main.ts:4. Small TypeScript positive, not a language battery. |
| lspSearch | 8.8 | 55% | Correct definition and three fixture references. Ignored-importer partiality disclosed. |
| graphBuild | 8.8 | 75% | Expected 5 nodes/7 edges; unchanged reuse and edited-scope rebuild work. Tiny TS graph. |
| graphQuery | 9.0 | 85% | Dependency, caller, all three symbol pages and stale/fresh recovery work. No large graph. |
| ghSearchRepo | 7.6 | 70% | Finds Vite/VitePress; empty/invalid paths useful. Multi-term ranking needs refinement. |
| ghSearchCode | 8.5 | 80% | Vue five-path result reconstructed in 3+2 pages; gh agrees. Default-branch index limits. |
| ghStructure | 8.0 | 70% | Vite/Vue trees and pinned refs work. Full tree continuations not exhausted. |
| ghGetFileContent | 8.4 | 70% | Frozen Vite block/range exactly agrees with GitHub. Large-file paging untested. |
| ghSearchHistory | 8.4 | 70% | Two path-scoped Vite commits agree with gh. Commit lane, not all PR/issue filters. |
| ghGetHistoryItem | 8.4 | 70% | Correct changed file; explicit 1-of-5 include filtering. PR/issue/patch paging untested. |
| artifactSearch | 8.8 | 70% | npm/PyPI versions, repos and tags verified. Broad npm searches noisy and provider-capped. |
| securityCheck | 8.8 | 70% | Affected/fixed npm versions and advisory ranges match OSV. Other ecosystems untested. |
| ghCloneRepo | 8.5 | 75% | Clean checkout reuse and new README-only tree work. Existing-cache dirty refusal untested. |
| clasify | 9.0 | 55% | Two approved synthetic judgments and source read cursor work. Calibration untested. |

The coordinator recomputed weighted means rather than copying worker composites. ghStructure
workflow is scored 8, not the worker's 7: an unwalked tree is a coverage gap, not a demonstrated
workflow defect. No aggregate average is given across these differently measured scopes.

## Deciding observations

### Local and graph workflows

The fixture declares `double` in math.ts:1, imports it in main.ts:1, and calls it at main.ts:4.
AST returned exactly that call; LSP resolved its definition to math.ts:1. The reference query
returned import/call/declaration while explicitly marking ignored-importer coverage partial.
No claim of globally complete references is made.

Graph build returned 2 files, 3 symbols, 1 import, 1 call, 3 containment edges and 2 uses:
5 nodes and 7 edges total. The caller of `math.ts#double` was `main.ts#run`. Symbols paged at
offsets 0, 1 and 2 with one distinct symbol each and final `hasMore:false`. After changing only
the synthetic label, `stale` identified math.ts as changed; rebuilding produced a new snapshot
and the next stale query reported fresh. First/repeat build times were 0.23/0.12 seconds in this
warm environment, not cold-start benchmarks or a measured competitive advantage.

### GitHub workflows

Vite ref `f898888222297aa56d6fc281c585d4d4e4d51f58` agreed with the independent commits API.
Its `packages/vite/src/node/server/index.ts` lines 506–510, including `createServer`, exactly
matched decoded GitHub contents. Vue `createRenderer` code search returned five paths; its
3+2 page union matched independent GitHub code search. Selected Vite commit
`e4babd7262256a3c5c383109eb7f7d489760faf5` contained the requested server file among five changed
files; the tool explicitly disclosed the include filter.

Repository search with `keywords:["vitejs","vite"]` did not place the expected repository in
its first three rows; simplifying to `["vite"]` did. This is observed search-ranking friction,
not proof the adapter is incorrect. Only early repository/tree pages were sampled.

### Packages, vulnerabilities and clones

zod 3.22.4 mapped to colinhacks/zod commit e7a9b9b3033991be6b4225f1be21da39c250bbb0;
requests 2.31.0 mapped to psf/requests tag v2.31.0. Registry metadata and remote tags agreed.
Broad npm search pages advance and disclose terminal provider caps. They were not exhausted;
no complete package-search or absence claim is made. An intentionally improbable multiword
query still returned provider relevance matches, not an empty result.

OSV independently returned GHSA-r5fr-rjxr-66jc and GHSA-xxjr-mmjv-4gpg for lodash 4.17.21,
matching securityCheck. For lodash 4.18.1, both reported no known matches. Thus the old
assumption that 4.17.21 remains unaffected is not valid in this observed provider data.
No known advisories is not a guarantee of safety.

Hello-World's reused clean checkout matched commit 7fd1a60b01f91b314f59955a4e4d4e80d8edf11d.
Its 13-byte README matched GitHub. A README-only request produced a distinct new tree location.
The existing user cache was not modified to induce a dirty-checkout refusal.

### Classification and cost

The user approved **exactly two inference calls**. Only synthetic facts were sent to the
configured classification provider: changing a service port requires restart; changing its
label does not. Inline answers were 0.98/0.05; source-backed answers were 0.98/0.03. The source
answer emitted a localFetch cursor, which returned the supporting lines. These probabilities
were sensible for these explicit examples; calibration was not evaluated.

Reported usage totaled **1,008 input tokens and 199 output tokens**. Wall times were 0.36 and
0.28 seconds. Exact monetary cost is unknown. Model discovery calls made no inference request.
No private repository content or credentials were submitted for classification.

## Failures, exclusions and remaining scope

- A suspected localSearch `status: empty` beside positive files did not reproduce in the
  coordinator's exact rerun. Expected hits were correct; no confirmed product defect is filed.
- One coordinator file-read verification used a mistyped SHA. The tool correctly refused it;
  corrected frozen-ref read succeeded. Both receipts are retained; this is an audit error.
- Near misses returned useful repairs: `targte` → `target`, `versoin` → `version`, and missing
  clone repo/security packageName examples. Numeric graph path 42 was explicitly coerced to
  string with a warning before notFound, rather than rejected by schema validation.
- No confirmed new defect requires an implementation follow-up from this limited round.
- All operation/view combinations, all languages/ecosystems, oversized/binary/Unicode stress,
  CLI/MCP parity, provider outage/rate-limit behavior, clone dirty refusal and broad performance
  comparisons remain untested. Relevant unfinished continuations are explicitly outside claims.
- Pre-existing repository changes were preserved. Audit fixtures and evidence are retained;
  normal graph/clone caches remain managed by the tools. No authored server is left running.

## Evidence

- [Request index and independent controls](../../../campaigns/2026-11-15-live-cli-use/integration/REQUESTS.md).
- [Raw coordinator receipts](../../../campaigns/2026-11-15-live-cli-use/integration/receipts/).
- [Known synthetic fixture](../../../campaigns/2026-11-15-live-cli-use/integration/fixture/).
- [Machine-readable dimensions and weights](ratings.json).
- Rubric: [RATINGS.md](../../../RATINGS.md).

Status: 17/17 tool positive paths exercised; CLI-only provisional ratings complete. No code fixes.
