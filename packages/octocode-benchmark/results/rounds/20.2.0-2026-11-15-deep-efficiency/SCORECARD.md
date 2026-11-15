# Octocode 20.2.0: deep efficiency and freshness review

Complete for the documented scope. Independent Astra review accepted the evidence and ratings; remaining limitations are explicit.

Octocode is strong at returning source-backed, complete evidence across one CLI/MCP contract. This round does not establish world-class performance across all tools. Expert rg, ast-grep and lean gh often return smaller selected answers; paid classification can lose to exact narrowing. Correctness fixes take precedence over apparent speed from stale answers.

The rebuilt host executable is `bfe156c11c87901b89ab6387efd778b1b864d0178e114fe36790a22d22be502b`, version 20.2.0. The [campaign](../../../campaigns/20.2.0-2026-11-15-deep-efficiency/README.md) contains questions, exact requests, raw responses, source manifests, failed attempts and receipts. Source authors and independent retesters worked in Sol/Astra lanes; root owned builds and integration. No publishing or commits were performed.

## Ratings and their scope

Scores follow the [seven-dimension rubric](../../../RATINGS.md): correctness 30%, schema/input 15%, output 15%, pagination 10%, workflow 10%, efficiency 15%, cache/freshness 5%. They are cumulative evidence ratings for named tested scopes, not a claim that every historical comparison was rerun on the new binary. The [previous scorecard](../20.2.0-2026-11-14-tool-ratings/SCORECARD.md) retains its original measurements and binaries.

C = correctness; S = schema/input; O = output; P = pagination; W = workflow; E = efficiency; F = cache/freshness. All dimensions are out of 10. U = unverified. Ratings are provisional with medium confidence across these cumulative scopes.

| Tool | Overall | C | S | O | P | W | E | F |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| localSearch | 8.8 | 10 | 9 | 8 | 10 | 9 | 6 | 9 |
| localFetch | 8.7 | 10 | 9 | 8 | 10 | 9 | 5 | 9 |
| structureSearch | 8.8 | 10 | 9 | 8 | 10 | 9 | 6 | 9 |
| astSearch | 8.4 | 9 | 9 | 8 | 10 | 9 | 5 | 9 |
| clasify | 8.1 | 9 | 8 | 8 | 10 | 8 | 5 | 9 |
| lspSearch | 8.1 | 9 | 8 | 8 | 9 | 8 | 6 | 8 |
| graphBuild | 8.5 | 9 | 8 | 8 | 9 | 8 | 8 | 9 |
| graphQuery | 8.3 | 9 | 8 | 8 | 9 | 8 | 7 | 9 |
| ghSearchRepo | 8.1 | 9 | 9 | 8 | 9 | 8 | 5 | 8 |
| ghSearchCode | 8.5 | 9 | 9 | 8 | 10 | 9 | 6 | U |
| ghStructure | 8.8 | 10 | 9 | 8 | 10 | 9 | 6 | U |
| ghGetFileContent | 9.1 | 10 | 9 | 9 | 10 | 9 | 7 | 9 |
| ghSearchHistory | 8.2 | 9 | 9 | 8 | 9 | 9 | 5 | U |
| ghGetHistoryItem | 8.7 | 10 | 9 | 8 | 10 | 8 | 6 | U |
| ghCloneRepo | 9.0 | 10 | 9 | 8 | 10 | 9 | 7 | 10 |
| artifactSearch | 8.2 | 9 | 8 | 8 | 9 | 9 | 6 | 8 |
| securityCheck | 8.5 | 9 | 8 | 8 | 9 | 9 | 8 | 8 |

Measured dimension weight is 95% for ghSearchCode, ghStructure, ghSearchHistory and ghGetHistoryItem; 100% for the other tools. This is not behavioral coverage. [Per-dimension evidence and measurement provenance](ratings.json) retain historical comparisons separately from fresh acceptance.

The main rating change is clasify 8.6 → 8.1: representative efficiency was previously unmeasured; its new limited task-family score is 5. Successful fixes restore correctness in the observed failure scopes; they do not automatically earn higher efficiency ratings. Security freshness 8 rests on deterministic final-source tests, not a CLI/MCP mutable-advisory replay.

A passed positive request is breadth acceptance, not proof of every option, provider state or operation. Historical long-value and nested pagination evidence remains historical unless this round explicitly replays it. Unknown freshness or comparative coverage keeps the relevant rating provisional. No unresolved wrong-answer defect may retain a score above 4.

## Confirmed improvements

- HTTP cache age now accounts for origin Date/Age, the successful physical response and body delay, and residence. Shared strict numeric/date parsing handles overflow, repeated/list Age, obsolete dates and pre-epoch values. Registry responses retain repeated cache-control headers, including no-store across 304. Independent runtime replay passed **70/70 scenarios**, **146 complete tool responses**, **122 origin requests** on actual CLI and fresh MCP processes, including persistent-home restarts. [Independent cache report](../../../campaigns/20.2.0-2026-11-15-deep-efficiency/provider-cache/POSTBUILD-INDEPENDENT-ACCEPTANCE.md).
- Rust LSP now uses client file notifications and includes ignored/generated sources in its freshness proof. Snapshot capture is ordered under the synchronization lock; notification succeeds before committing state. Failed or cancelled initial capture retires the unproved connection. Large source files are streamed and cancellation checked, with no arbitrary evidence-size cutoff. Historical page fingerprints cannot be replaced by a later metadata-only observation. [Source review](../../../campaigns/20.2.0-2026-11-15-deep-efficiency/rust-lsp/INDEPENDENT-REVIEW.md).
- Readable multiline search/AST hits share one filename while retaining every source line, Unicode character and column. A regression failed before repair and passed afterward. The PCRE recovery hint now names the actual `regexEngine` field.
- Canonical skill guidance now distinguishes supported, explicitly disclosed input repairs from unsupported inputs. Generated skill copies were rebuilt. Core schema, descriptions and wire contracts did not change; runtime rebuilding required no contract regeneration.

Rust API review favored the locked HTTP crate's infallible numeric header conversion, one shared HTTP-date parser, bounded working memory for complete streaming hashes, and the existing cancellable blocking-work boundary. Three narrowly scoped lint exceptions document protocol-duration conversions; no warning baseline or coverage floor was lowered. [Decisions and rationale](../../../campaigns/20.2.0-2026-11-15-deep-efficiency/DECISIONS.md).

## Efficiency and workflow findings

Final three-repetition warm medians on the same 40-file corpus, with normal memory pressure and load about 1.21 on 12 cores:

| Selected task | Node CLI | Native CLI | Persistent MCP | Expert CLI | Natural CLI bytes: Octocode / expert |
|---|---:|---:|---:|---:|---:|
| S1 file membership | 50.32 ms | 12.94 ms | 4.69 ms | rg 5.44 ms | 443 / 370 |
| A1 structural match | 54.69 ms | 16.89 ms | 8.67 ms | ast-grep + projection 12.67 ms | 301 / 35 |

Persistent MCP measures a warm tool call; Node/native CLI include launching the requesting process against a warm backend; expert CLI starts its process or pipeline. Byte figures include natural CLI stdout and stderr. Selected facts match; the representations differ. All 61 sample output/exit checks and all 40 corpus hashes passed. Comparable before/after arms remain similar; three short repetitions do not prove a causal speedup or universal ranking. The old override route's first repetition had unproven warmth and is excluded from persistence-matched claims. [Timing comparison and limits](../../../campaigns/20.2.0-2026-11-15-deep-efficiency/local-efficiency/postbuild-timing/comparison.json).

The multiline renderer case reduced complete CLI output **663 → 435 bytes (34.4%)**, retaining exact Unicode source, line numbers and metadata. Its stdout changed 387 → 159 bytes; stderr remained 276. All 17 preservation checks passed; structured JSON/MCP data did not change. [Local report](../../../campaigns/20.2.0-2026-11-15-deep-efficiency/local-efficiency/REPORT.md).

In the real native Rust workspace, all six definition/reference chains matched the source oracle, including complete three-page reference walks. Cold definition took 4.917 s; the first reference chain's request time was 0.321 s. Warm definitions were about 12 ms and reference request-time sums 104/108 ms. Native reference-chain CPU was 2.19 s initially and 0.51/0.53 s warm; these aggregate counters do not isolate fingerprint-walk cost. Monitoring and receipt overhead are excluded from request-time sums and retained in separate full-chain timings. The analyzer was still priming; process-cold does not mean OS-cache-cold, and CPU snapshots slightly widen the measured interval. No build/check or compiled output was observed. The first interrupted attempt was an over-conservative process-monitor false positive and supplies no latency result; the corrected attempt is retained separately. [Workspace results and attribution limits](../../../campaigns/20.2.0-2026-11-15-deep-efficiency/rust-lsp/REAL-WORKSPACE-RESULTS.md).

The classification question used 15 complete existing TypeScript files and all 33 callback sites. Both sparse and dense selections matched the independent source oracle. Sparse classified search plus deciding source reads returned **2,566 characters** and used **13,638 provider input tokens**; the strongest post-hoc exact rg filter returned the same two complete callbacks in **531 characters**. That pattern was designed after seeing this corpus; it is not a universal semantic parser or a held-out benchmark. Dense classification barely reduced output. Two independent questions over the same source used **47.8% fewer provider input tokens when batched**; latency improvement was not established. Saved response-page replay made no further physical provider requests. These classification measurements belong to the pre-rebuild `4e2052ae…` binary. [Complete classification report](../../../campaigns/20.2.0-2026-11-15-deep-efficiency/classification/REPORT.md).

For GitHub, equivalent cache policy matters. The small two-repository metadata comparison against `gh api --cache 60s` found comparable warm wall times and smaller projected gh output; it does not support a universal Octocode speed win. Owned conditional-request checks distinguish 200 bodies, 304 responses and the caller's saved-body work. Historical Graphify/direct-LSP comparisons retain their original scope and binary; they are not new measurements of this release.

The native MCP serves common instructions once at initialization. This host's tool metadata representation repeats a 3,887-character common block across 14 entries, adding 50,531 characters beyond one copy. Actual model-loaded tokens and billing are unknown. Changing core descriptions to compensate would target the wrong layer. [Prompt/context evidence](../../../campaigns/20.2.0-2026-11-15-deep-efficiency/local-efficiency/prompt-context-review.json).

## Practical strengths, costs and gotchas

| Area | Strength | Cost or limitation |
|---|---|---|
| Research workflow | One contract joins discovery, exact reads, syntax, semantics, history and packages | A known narrow question can be cheaper through a projected expert command |
| Evidence | Source identities, partial states and executable continuations | Metadata and repeated envelopes increase output; all required pages must be counted |
| Local execution | Persistent MCP amortizes startup | Node CLI startup materially changes short-task comparisons |
| Pagination | Complete selected evidence is retained, including oversized atomic values | Finish response pages before tool-level pages; do not edit emitted cursor calls |
| Rust semantics | Real analyzer identity and source positions, with explicit stale snapshots | Cold indexing costs seconds; warm native-workspace reference requests cost about 0.1 s here |
| Providers | Policy-aware persisted cache and conditional validators | A disclosed heuristic search window is not a guarantee of current global index contents |
| Classification | Typed scouting and shared-source batching | Paid judgments require source verification; fresh evaluation bills again, saved pages do not |
| Graphs | Reusable structural snapshots and witnesses | Syntax edges are candidates; use LSP to confirm semantic callers or unused-code claims |

Use explicit scope and the lean representation that answers the question. For rg, supply `.` in subprocess benchmarks, `-F` for literals and `-l` for file membership; align hidden/ignored policy. For ast-grep, compare structural facts and project only the selected ranges. For gh, match refs, selected fields and cache policy. For Octocode, batch independent questions, prefer persistent MCP during repeated work, and include deciding reads and every required continuation in total workflow cost.

## Validation and remaining work

The final source passed **8,696 Rust tests, 16 skipped**, lint, skill checks, host release build, both Node launcher builds and output checks. The build/test guard covers **3,311 source inputs with no drift**. [Build provenance](../../../campaigns/20.2.0-2026-11-15-deep-efficiency/build-provenance.json), [full tests](../../../campaigns/20.2.0-2026-11-15-deep-efficiency/rust-full3.json), [lint](../../../campaigns/20.2.0-2026-11-15-deep-efficiency/rust-lint4.json).

Fresh all-tool acceptance: **34 positive starts + 34 meaningful invalid requests**, **42 complete chains / 131 positive page calls**, exact effective schema/description/output-reference parity for all 17, and no unreachable required continuation observed. Independent source validation is **16 PASS / 1 LIMITED**: live mutable repository metadata lacks a fresh independent oracle. Pinned history reconstructs all 72 selected commits; selected source, patches, sparse clone files and provider records match their stated oracles. [All-tool report](../../../campaigns/20.2.0-2026-11-15-deep-efficiency/all-tools/REPORT.md).

Independent Rust replay: R1–R5 and actual Node CLI C1–C5 all pass, all 12 operations agree across surfaces, unopened/ignored/hidden/large-source mutations return current evidence, unchanged historical pages survive settling and an intervening query, and changed source is explicitly refused. Emitted restart calls were executed unchanged and recovered current evidence. [Rust replay](../../../campaigns/20.2.0-2026-11-15-deep-efficiency/rust-lsp/REPLAY-PLAN-results.md).

An earlier full-suite classification request-recorder failure produced an empty body and was not reproduced. Stronger method/path/body diagnostics preserve every recorded request and all original assertions; focused and subsequent full runs passed. Its original cause remains unknown. [Diagnostic record](../../../campaigns/20.2.0-2026-11-15-deep-efficiency/classification/REQUEST-RECORDER-FAILURE.md).

Measured next experiments are specific: coalesce identical in-flight provider requests (five cold identical queries made five GETs); reduce repeated source reads only while preserving raw-byte/race proofs; measure and reduce full Rust server-root walk cost without skipping compiled sources; remove optional metadata duplication; and add held-out classification task families and calibration. None is claimed as an implemented speedup. All owned runtime processes and temporary fixtures were cleaned. Final source/binary verification found no drift in 3,311 guarded inputs and no extra Rust source inputs. Canonical pruning passed; disk availability afterward was 485 GiB. [Independent final review](../../../campaigns/20.2.0-2026-11-15-deep-efficiency/final-review.md), [final source/binary check](../../../campaigns/20.2.0-2026-11-15-deep-efficiency/final-source-binary-check.json), [cleanup/prune receipt](../../../campaigns/20.2.0-2026-11-15-deep-efficiency/prune-rust.json).

Fresh CLI/MCP processes were verified against the rebuilt executable. Pre-existing sessions owned by other clients were preserved; they need reconnection to load the new executable.
