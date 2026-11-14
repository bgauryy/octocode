# Scorecard: Octocode 20.2.0, 2026-11-14 — advanced comparisons

**Selected evidence was correct across the local, AST, graph and LSP comparisons. Expert shell
and gh projections were smaller; direct LSP was faster for raw locations. Octocode's graph
queries were faster in the measured cases and exposed more call-site detail. Warm GitHub
speed advantages did not survive every explicit gh cache comparison.** There is no overall
product winner or blind-agent superiority claim.

One frozen question exceeded Octocode's representation: its annotated-tag lookup exposed the
peeled commit but not the intermediate tag-object SHA. That question is a capability loss,
excluded from efficiency ratios. No product implementation defect was established in this round.

| Round | Value |
|---|---|
| Version | `octocode 20.2.0` |
| Binary | Host-only optimized release, SHA-256 `0dd294156a9799c7fd0aec869415b9e4187cac7fe07de536969d664229670d2d`, 73,681,248 bytes |
| Source | `73cddf5b32ea463dd4134a2749af0bef008300aa`, shared dirty tree; 2,943 native/contract file fingerprints unchanged during build |
| Build | Native package `build`, release opt-level 3, thin LTO, one compiler job; 629.9 seconds; build and `check-outputs` exited 0 |
| Previous version compared | None. Fresh competitor capability round, explicitly requested; prior audit ratings are not carried over |
| Surfaces | Pinned local CLI through the Node launcher, fresh stdio MCP, supplementary direct native CLI |
| Agents and models | Root integration; two GPT-6.1 Sol lanes; GPT-6 Astra graph/LSP lane and independent evidence review |
| Checked by / signed off by | Lane owners plus root source/page checks; independent GPT-6 Astra approved scoped claims ([review](../../../campaigns/20.2.0-2026-11-14-advanced/independent-review.md)) |

The temporary pin was removed after acceptance. [Build provenance](../../../campaigns/20.2.0-2026-11-14-advanced/build-provenance.json)
and [build log](../../../campaigns/20.2.0-2026-11-14-advanced/build-release.log) retain the identity.
No product code or generated contract changed for these comparisons. The earlier
[all-tool and Rust/API audit](../20.2.0-2026-11-14-final-verification/REPORT.md)
is separate evidence, not a new release-profile test suite.

## Verdict

| Area | Verdict | Scope or exception |
|---|---|---|
| Core tools | Selected comparisons passed, with one capability loss | G4 requires a gh supplement for the intermediate tag object |
| Security | Not assessed as a security suite | Isolated homes/install, public sources, process cleanup only |
| Skills | Applied, activation not scored | Local development, orchestration, graph/Rust and documentation guidance |
| Release gates | Build/output checks passed; full release verdict not assessed | No product change; earlier full audit remains separate |
| Benchmark | Mixed by question and representation | New competitor runs, not previous-version regression testing |
| **Overall** | **Scoped capability comparison complete** | No global readiness or superiority claim |

### Method and accounting

Questions, fixture hashes and public refs were frozen before answer discovery. Sol covered eight
local cases and five GitHub/hybrid cases; Astra covered a synthetic JavaScript project, eight
pinned Python source files and the same installed TypeScript language server through direct LSP.
Graphify was the official `graphifyy` 0.9.108 package, installed temporarily from wheels, with
code-only extraction, one worker and no clustering or model calls.

Three alternating paired repetitions were used for measured comparisons. Executable benchmark
windows were serialized, while background applications remained active. Host load varied
substantially: the first local window was around 28–32 on 12 cores; its later natural-text window
was around 9.54. These are observed shared-host samples, not controlled performance guarantees.
First calls, repeated calls, server reacquisition, graph builds and clone setup are separate.
No user cache was cleared; process warmth alone is not evidence of a provider cache hit.

Characters mean Unicode code points. CLI text counts stdout **and** stderr. JSON, MCP content,
raw LSP wire results and derived semantic projections are separate representations. Logical
request costs exclude repeated harness environment/pin bookkeeping; setup is counted separately.
Exact total model-visible setup is unknown because some initial help captures were truncated by
the host; complete saved help is not claimed to have all reached the model. `didOpen` source
traffic is not model context. No provider-token or dollar saving is asserted.

All required pages are included in accepted answers. Invalid harness attempts remain in receipts:
an initial MCP framing error, environment/helper setup errors and a GitHub cache arm that missed
a continuation. The incomplete cache arm was excluded and rerun completely. Default CLI text,
capture-free AST output and anchored rg output were checked so avoidable verbosity did not
become the primary baseline. Exact commands and all samples are in the lane reports:

- [Local report and metrics](../../../campaigns/20.2.0-2026-11-14-advanced/local/REPORT.md).
- [GitHub report and metrics](../../../campaigns/20.2.0-2026-11-14-advanced/github/report.md).
- [Graphify and LSP report and metrics](../../../campaigns/20.2.0-2026-11-14-advanced/semantic/REPORT.md).

## Core tools

Ratings are 1–5 for **these selected cases**, not product-wide scores. 5 means strong evidence
in the tested dimension; 4 means correct with modest friction; 3 means material tradeoffs;
2 means a consistent cost disadvantage. NC means not checked, NA not applicable. Pagination
gets a rating only where continuations were actually exercised. No delta is asserted against
an earlier version. Security and broad stress are not inferred from successful ordinary calls.

| Tool | Schema/input | Selected output | Workflow | Efficiency | Pagination | Stress | Security | Reason below 4 or limit |
|---|---:|---:|---:|---:|---:|---:|---|---|
| localSearch | 4 | 4 | 4 | 2 | 5 | 5 | NC | Larger text and Node launch cost; long Unicode search windows exhausted |
| localFetch | 4 | 4 | 4 | 2 | 5 | 5 | NC | Complete large blocks; tiny pages amplify round trips |
| structureSearch | 4 | 4 | 4 | 2 | NC | NC | NC | Correct complete byte/line rankings; expert projection smaller |
| astSearch | 4 | 4 | 4 | 2 | NC | NC | NC | Syntax parity, including deliberate shadows; no identity claim |
| lspSearch | 4 | 4 | 4 | 3 | NC | NC | NC | Direct LSP faster; enriched evidence and lifecycle management add cost |
| graphBuild | 4 | 4 | 4 | 4 | NA | NC | NC | Two small scopes only; graph models differ |
| graphQuery | 4 | 5 | 4 | 3 | 5 | NC | NC | Faster selected queries, but substantially longer text |
| ghSearchRepo | NC | NC | NC | NC | NC | NC | NC | Not in this fresh comparison |
| ghSearchCode | 4 | 4 | 4 | 3 | NC | NC | NC | Search is discovery; exact ref reads establish source |
| ghStructure | 4 | 4 | 4 | 3 | 5 | NC | NC | G4 tag-object representation gap; tree correctness passed |
| ghGetFileContent | 4 | 4 | 4 | 3 | NC | NC | NC | Correct source; gh projections smaller |
| ghSearchHistory | 3 | 4 | 4 | 3 | NC | NC | NC | Observed field-name recovery; no schema defect established |
| ghGetHistoryItem | 4 | 4 | 4 | 3 | NC | NC | NC | Complete scoped patch; expert projection smaller |
| ghCloneRepo | 4 | 4 | 4 | 3 | NA | NC | NC | Setup amortization depends on query and cache policy |
| clasify | NC | NC | NC | NC | NC | NC | NC | No model/classification comparison |
| artifactSearch | NC | NC | NC | NC | NC | NC | NC | Not in this fresh comparison |
| securityCheck | NC | NC | NC | NC | NC | NC | NC | Not in this fresh comparison |

### Pagination and evidence checks

| Tool / selected view | Result | Evidence |
|---|---|---|
| localSearch long-line content | 7 windows across 5 response pages, complete | Frozen Unicode fixture and source oracle |
| localFetch Unicode full content | 42 pages on CLI and 42 on fresh MCP; exact 35,217-byte source reconstructed | SHA-256 `37011ef9be591dad8cffca4468816abe73473467f620ed36495e254caf33d9e8` |
| localFetch large declaration | Complete 454-line declaration; attached preceding comment disclosed | Exact AST-derived range versus source; no guessed end |
| graphQuery witnesses | 12 unique synthetic witnesses across 2 pages; 19 real call sites | Source and normalized edge/site oracles |
| ghStructure tree | All 46 ordered path/byte rows across 3 stress pages; no duplicates | Provider `truncated:false`, terminal `hasMore:false` |
| localFetch cloned GitHub source | Complete 873-line resolveConfig declaration across 2 pages | Exact equality with immutable remote source |

Root independently reconstructed the Unicode hash and checked every next-input against its
preceding emitted cursor, and independently compared all 46 tree rows:
[local review](../../../campaigns/20.2.0-2026-11-14-advanced/root-local-review.json),
[GitHub review](../../../campaigns/20.2.0-2026-11-14-advanced/root-github-review.json).
Tools marked NC above were not given a pagination score merely for returning one complete page.

## Benchmark

### Local tools versus expert rg, ast-grep and file utilities

Both sides passed all selected evidence checks. The main table uses natural CLI text, both
streams, against expert projected text. Paths, line anchors and selected source remain present.
Deep fixture prefixes contribute to observed overhead; these counts are not a universal ratio.

| Case | Selected question | Output characters Octocode / expert | Median ms Octocode / expert |
|---|---|---:|---:|
| L1 | OR occurrences and matching-line counts | 776 / 160 | 49.02 / 21.00 |
| L2 | Whole-file A AND B NOT C | 163 / 42 | 49.70 / 22.37 |
| L3 | Multiline PCRE2 match with source anchors | 520 / 50 | Not paired for final anchored counterpart |
| L4 | All seven Unicode token matches | 347 / 124 | 46.44 / 6.99 |
| L5 | Complete byte and line-count rankings | 2,749 / 837 | 46.32 / 26.98 |
| L6 | Nested Rust and TypeScript calls | 876 / 196 | 48.87 / 23.12 |
| L7 | Whole declaration over 400 lines | 9,158 / 7,186 | 45.65 / 19.52 |
| L8 | Real-source discover → AST → exact read | 2,190 / 398 | 148.56 / 28.52 |

The expert output was smaller in all eight cases. The final L3 line-anchor refinement was
validated in three output-only replays after timing; its earlier unanchored speed is not
substituted here. L7 includes an attached comment in Octocode's representation. The earlier
JSON timing window had unstable L1/L5 ordering under high load and is retained separately.
The 42-page full-file stress is not pooled with the lean L4 token question.

On the same L2 request, three paired supplementary samples gave median **60.31 ms through Node
versus 14.99 ms direct native**. A separate pair gave **63.89 ms Node versus 3.55 ms persistent
MCP**, with about 80.66 ms spawn-to-initialize setup and a 13.60 ms first MCP request. These
measure different interfaces, not a cache-hit proof or an engine-wide speed multiplier.

### GitHub and hybrid workflows versus gh and gh-lean

Fixed refs: Vite `f898888222297aa56d6fc281c585d4d4e4d51f58`; cli/cli
`b300f2ec7ec9dc9addc39b2ad88c54097ded7ca0`; git/git v2.47.0 annotated tag
`4ec4435df758668055cc904ef64c275bc8d1089b` → commit
`777489f9e09c8d0dd6b12f9d90de6376330577a2`.

| Case | Evidence / logical calls Octocode : gh-lean | Output characters Octocode / gh-lean | Median ms Octocode / gh-lean |
|---|---|---:|---:|
| G1 | Declaration, one caller and identity; 3 : 3 | 7,435–7,478 / 4,851 | 237 / 1,349 |
| G2 | Three commits, complete scoped patch, changed declarations before/after; 4 : 4 | 14,819 / 12,040 | 307 / 1,546 |
| G3 | All 46 ranked tree rows, explicit limit 100; 1 : 1 | 2,522 / 2,211 | 77 / 385 |
| G4 | Full annotated-tag object chain | Octocode capability gap | Excluded |
| G5 | Three complete declarations, 14 + 5 + 873 lines; 4 : 3 | 35,228 / 29,221 | 312 / 1,186 |

These gh-lean rows have no explicit cache flag; provider cache state is otherwise unknown.
G1's Octocode samples were **2,106, 226 and 237 ms**: the first workflow was slower than its
paired gh run, while later search responses explicitly marked cached observations. Report the
first request as well as the median. G3's forced limit-20 pagination stress is separate from
the explicit limit-100 efficiency arm. G5 costs exclude clone/discovery setup, which follows below.
Ordinary targeted gh and lean gh coincide on several cases; no artificial verbose dump was added.

With `gh api --cache 5m`, the complete G5 repeats were median **193 ms**, versus **288 ms** for
the complete local Octocode reads. Both included the long declaration. Three fresh sparse clone
setups took **2,326 / 2,241 / 2,333 ms**. A projected break-even against reads without an explicit
gh cache flag is about 2.66 three-declaration sets, excluding discovery; **no break-even was observed against
the faster explicit gh cache arm**. Clone for local AST/LSP/graph needs or varied repeated work,
not because repeated immutable reads are assumed to require network trips. The official
[gh API manual](https://cli.github.com/manual/gh_api) documents cache and pagination separately.

### Graphify and direct LSP

Both graph tools recovered all six selected call edges, six module-import edges, the same
16 real caller functions and the selected one- and two-hop paths. Octocode exposed all
19 individual real call sites; Graphify's examined graph and `explain --all` retained one
location per caller-to-target edge. This is evidence granularity, not missing caller nodes.

| Selected graph query | Median ms Octocode / Graphify | Native text characters Octocode / Graphify |
|---|---:|---:|
| Synthetic callers (depth 3) | 79.18 / 237.84 | 1,037 / 116 |
| Real 16 callers | 66.89 / 216.47 | 4,097 / 674 |
| Real two-hop path | 85.46 / 278.40 | 1,256 / 167 |

Octocode was faster in all nine selected query pairs; Graphify was smaller in all three answers.
Build costs and artifact sizes are in the semantic report, without a same-model throughput or
compression claim: Graphify includes configuration and other graph relations absent from the
native scope. The tested native import-cycle classification does not prove runtime execution.

Direct LSP and both Octocode surfaces agreed on eight reference locations, alias and shadow
definitions, and two incoming callers. Warm median ms MCP/direct were **7.99/3.17** for
references, **3.17/0.76** for definition, and **9.47/1.78** for incoming callers. Direct LSP's
setup was 558.71 ms. Octocode's first measured reference request reacquired an idle server and
took 789.58 ms; it remains separate from the three warm repeats. Raw locations are smaller
than Octocode's source, roles and diagnostics; a common relative-path reference projection is
336 characters on either side. Both use TLS 6.0.2 / TypeScript 6.0.3.

The fixture intentionally retains TypeScript diagnostic 7006. Selected answers match the source
oracle, but clean global analysis is not claimed. Diagnostic metadata varied between saved CLI
and MCP responses; absent metadata is not treated as proof of complete semantic coverage.
`rg -w` remains a spelling comparison, not an alternative semantic resolver: against this
reference set it had four true positives, eight false positives and four missed renamed references.

## Workflows, caching and optimizations

| Action | Measured reason / status |
|---|---|
| Reuse persistent MCP for many small local calls | Measured L2 warm request 3.55 ms; fresh Node client startup remains even with a warm daemon |
| Use the native executable when comparing engine/process costs | Measured Node/direct-native gap; keep launcher costs in the ordinary CLI result |
| Request the evidence actually needed | `matchOnly`, paths/counts and capture-free AST avoid unasked data; full reads still finish every page |
| Derive declaration ranges with AST on the expert side | Removes guessed windows and makes the >400-line comparison fair |
| Avoid repeated reads of already-present proof | Removed a duplicate G1 import read after proving the same immutable line was already returned |
| Tune page size separately from completeness stress | 42 tiny fetch pages were correct but expensive; ordinary work should not inherit the stress window |
| Cache immutable GitHub reads deliberately | Explicit gh cache reverses the tested hybrid speed conclusion; use SHA/ref provenance and retain cache policy |
| Reduce repeated paths and optional metadata in narrow answers | Candidate product optimization, not implemented: L3 repeats its long fixture path on each line; L8 text can exceed JSON because of paths and suggestions |
| Keep graph freshness and site granularity visible | Both old graphs returned the old path; Octocode disclosed stale state, then both updated correctly |

No evidence may be removed to meet a character target. A future output change should share
path/provenance context without losing source anchors, diagnostics or executable continuations.
It needs byte/character evidence before and after, plus the existing exact reconstruction tests.
New caching should be justified by observed reuse and tested for edits, restored files, refs,
errors and stale snapshots; lower elapsed time alone is insufficient.

Observed invalidation checks: local A/B membership changed immediately and returned after exact
byte restoration; both graph updates changed the path from one to two hops; direct LSP didChange
changed references 8 → 6 → 8. All original source hashes were restored. Before graph refresh,
Octocode named the changed file and supplied recovery; the tested Graphify path command gave no
stale warning. This does not assert that Graphify lacks other watch/update facilities.

## Security

No security score is assigned. Public refs and owned fixtures were used; Graphify ran locally
without global hooks or model egress. Existing credentials were used without logging secrets.
Only owned process IDs and clean owned clones were eligible for cleanup. Redaction, symlink,
prompt-injection and provider-refusal suites were not rerun here.

## Skills

Local skills guided development/builds, live orchestration, graph/semantic checks, Rust practices
and documentation. Skill activation quality and the previous Rust/API ratchet were not re-scored.
The comparison owner page was corrected: direct LSP is now the semantic baseline; whole-function
shell reads use AST-derived ranges; Graphify is a measured peer with source witnesses; complete
rg examples no longer suggest `-m`/`head` truncation.

## Release gates

| Gate | Result |
|---|---|
| Host optimized native build | Passed, exit 0 |
| Built outputs | Passed, exit 0 |
| Source consistency during build | 2,943 fingerprints unchanged |
| Product tests/lint/typecheck | Not rerun: no product source or contract edits in this round |
| Documentation verification | Passed, exit 0; [log](../../../campaigns/20.2.0-2026-11-14-advanced/docs-verify-final.log) |
| Skills check | Passed, exit 0; [log](../../../campaigns/20.2.0-2026-11-14-advanced/skills-check-final.log) |
| Final source consistency | All 2,943 fingerprints still unchanged; [proof](../../../campaigns/20.2.0-2026-11-14-advanced/final-source-proof.json) |
| Owned cleanup | Pin, runtime homes, clean clones, temporary Graphify installation and owned processes removed; [root receipt](../../../campaigns/20.2.0-2026-11-14-advanced/root-cleanup.json) plus lane receipts |
| Safe Rust prune / disk | Prune refused active foreign FreeLlama Cargo processes; preserved them. 492 GiB free. [Prune log](../../../campaigns/20.2.0-2026-11-14-advanced/prune-rust.log), [disk](../../../campaigns/20.2.0-2026-11-14-advanced/disk-final.txt) |

The documentation gate initially interpreted an inline Go type in a shared skill page as an
Octocode contract field. Clarifying it as “Go's request context” preserved the cancellation
guidance and passed verification; the skill checks then passed. This was a wording correction,
not a runtime change or a claim that the documentation validator was redesigned.

## Defects and limitations

| ID | Finding | Classification / handling |
|---|---|---|
| C1 | Annotated tag-object SHA absent from G4 Octocode answer | Capability gap; gh supplement needed; no efficiency win counted |
| O1 | Repeated path and optional metadata overhead | Optimization candidate; selected evidence still complete |
| U1 | LSP diagnostic metadata varied across saved responses | Observed limitation, root cause unknown; no clean-coverage claim |
| H1 | One supplementary cache harness skipped a required page | Invalid measurement excluded; complete three-pair rerun accepted |

No product fix is claimed for C1, O1 or U1. Building a broader schema feature or changing cache
semantics without a diagnosed defect would exceed the evidence from these selected comparisons.

## Not checked

No blind research-agent trial, previous-version benchmark, large-repository scalability,
cross-platform build, all-language semantic recall, classification/model cost, Graphify hosted
or multimodal features, graph co-change/drift, registry/security benchmark, or full release gate
suite was performed in this round. Small selected fixtures cannot establish general product
rankings. Setup/help totals, timing variability, diagnostic metadata and unknown provider cache
state remain explicit limits. All retained accepted answers exhausted their selected evidence.
