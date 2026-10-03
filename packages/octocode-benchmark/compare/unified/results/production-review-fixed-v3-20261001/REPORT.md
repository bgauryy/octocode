# Unified benchmark — run `production-review-fixed-v3-20261001`

Workers: `octocode`, `rg-gh` · model claude-sonnet-5-5 · 30 questions × 1 pass · judge claude-opus-5-5 (blinded X/Y, both orders, tie-break when spread > 2).
Build: MCP dist sha256 `d0dbe479306c` · Claude Code 2.1.286 (Claude Code).

Quality 0–10 = correctness 0–5 + completeness 0–3 + evidence 0–2. Tokens: total = all input kinds + output; research is an estimate: total − (first-request context × requests). Weighted tokens are unknown without a frozen model tariff and verified cache TTL. Cost and tokens cover the worker Claude session; classifier provider totals are separate/unknown. Efficiency = quality per 10k total tokens; weighted efficiency = quality per 10k weighted tokens.

## Totals

| worker | mean quality | median quality | total tokens | weighted tokens | research tokens | output tokens | requests | tool calls | cost | time | efficiency | weighted efficiency |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| octocode | 8.38 | 8.5 | 2361.2k | — | 501.6k | 60.9k | 113 | 125 | $2.25 | 9.1 min | 1.07 | — |
| rg-gh | 8.97 | 9.0 | 940.5k | — | 398.5k | 50.7k | 112 | 83 | $1.71 | 8.7 min | 2.86 | — |

Per-question ratio octocode/rg-gh: total tokens mean 2.70× / median 2.60×; weighted tokens mean —× / median —×; research tokens mean 1.37× / median 1.06×; cost mean 1.32× / median 1.23×. Quality delta (octocode − rg-gh): mean -0.59, wins/ties/losses 3/14/13.

## Fixed overhead ("reply OK" probe)

| worker | first-request context | tools offered | isolation probe |
|---|--:|--:|---|
| octocode | 16.4k | 13 | pass |
| rg-gh | 4.7k | 1 | pass |

## By category

| category | n | octocode quality | octocode tokens | octocode research | rg-gh quality | rg-gh tokens | rg-gh research |
|---|--:|--:|--:|--:|--:|--:|--:|
| github | 10 | 7.95 | 70.9k | 16.8k | 8.60 | 27.2k | 11.9k |
| local | 20 | 8.60 | 82.6k | 16.7k | 9.16 | 33.4k | 14.0k |
| github-bug-rca | 2 | 8.50 | 68.1k | 10.8k | 8.50 | 21.6k | 7.3k |
| github-code-research | 3 | 7.67 | 59.7k | 10.5k | 9.33 | 29.1k | 11.7k |
| github-pr-review | 5 | 7.90 | 78.7k | 23.0k | 8.20 | 28.2k | 13.9k |
| local-impact | 2 | 9.00 | 70.6k | 12.8k | 9.38 | 21.8k | 7.1k |
| local-locate | 1 | 9.00 | 72.6k | 6.7k | 9.00 | 18.0k | 3.5k |
| local-semantic | 6 | 8.58 | 94.4k | 22.9k | 9.42 | 33.1k | 14.4k |
| local-trace | 11 | 8.50 | 79.3k | 14.9k | 9.00 | 37.2k | 15.9k |

Token columns are per-question means.

## Per question

Each cell shows octocode / rg-gh.

| q | category | quality | total tokens | weighted tokens | research tokens | tool calls | time | cost | efficiency |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|
| G01 | github-pr-review | 8.0 / 7.0 | 129.5k / 48.6k | — / — | 47.5k / 29.5k | 5 / 3 | 31s / 31s | $0.130 / $0.104 | 0.62 / 1.44 |
| G02 | github-pr-review | 8.0 / 8.0 | 88.6k / 28.0k | — / — | 23.1k / 13.7k | 3 / 2 | 19s / 18s | $0.089 / $0.074 | 0.90 / 2.86 |
| G03 | github-pr-review | 9.0 / 8.0 | 41.4k / 18.0k | — / — | 8.6k / 8.4k | 1 / 1 | 12s / 12s | $0.059 / $0.054 | 2.18 / 4.45 |
| G04 | github-pr-review | 8.0 / 8.5 | 55.7k / 20.3k | — / — | 6.6k / 6.0k | 2 / 2 | 12s / 13s | $0.048 / $0.041 | 1.44 / 4.20 |
| G05 | github-code-research | 8.0 / 9.0 | 101.4k / 31.0k | — / — | 19.4k / 12.0k | 4 / 3 | 21s / 19s | $0.081 / $0.055 | 0.79 / 2.90 |
| G06 | github-code-research | 7.0 / 9.0 | 37.9k / 27.9k | — / — | 5.1k / 8.8k | 2 / 3 | 12s / 17s | $0.046 / $0.049 | 1.85 / 3.23 |
| G07 | github-code-research | 8.0 / 10.0 | 39.8k / 28.5k | — / — | 7.0k / 14.2k | 2 / 3 | 19s / 19s | $0.060 / $0.066 | 2.01 / 3.50 |
| G08 | github-bug-rca | 8.0 / 9.0 | 54.9k / 20.0k | — / — | 5.7k / 5.7k | 3 / 2 | 11s / 13s | $0.045 / $0.038 | 1.46 / 4.51 |
| G09 | github-bug-rca | 9.0 / 8.0 | 81.4k / 23.2k | — / — | 15.9k / 8.9k | 3 / 2 | 19s / 20s | $0.081 / $0.051 | 1.11 / 3.45 |
| G10 | github-pr-review | 6.5 / 9.5 | 78.4k / 26.2k | — / — | 29.2k / 11.8k | 2 / 2 | 19s / 18s | $0.109 / $0.066 | 0.83 / 3.63 |
| L01 | local-trace | 9.0 / 10.0 | 53.5k / 20.9k | — / — | 4.1k / 6.4k | 2 / 2 | 12s / 13s | $0.044 / $0.043 | 1.68 / 4.78 |
| L02 | local-locate | 9.0 / 9.0 | 72.6k / 18.0k | — / — | 6.7k / 3.5k | 4 / 2 | 16s / 12s | $0.052 / $0.033 | 1.24 / 4.99 |
| L03 | local-trace | 9.0 / 9.0 | 107.8k / 22.0k | — / — | 25.5k / 7.4k | 9 / 2 | 23s / 14s | $0.099 / $0.049 | 0.83 / 4.10 |
| L04 | local-trace | 8.0 / 8.5 | 142.6k / 84.3k | — / — | 43.6k / 50.1k | 9 / 6 | 27s / 32s | $0.124 / $0.103 | 0.56 / 1.01 |
| L05 | local-impact | 9.0 / 8.8 | 54.8k / 20.8k | — / — | 5.3k / 6.1k | 2 / 2 | 10s / 12s | $0.044 / $0.039 | 1.64 / 4.21 |
| L06 | local-trace | 9.0 / 9.0 | 55.5k / 29.9k | — / — | 6.1k / 10.5k | 2 / 3 | 17s / 16s | $0.048 / $0.048 | 1.62 / 3.01 |
| L07 | local-trace | 8.5 / 10.0 | 61.8k / 25.2k | — / — | 12.3k / 10.6k | 5 / 2 | 17s / 17s | $0.071 / $0.056 | 1.38 / 3.97 |
| L08 | local-trace | 8.0 / 9.0 | 80.6k / 40.4k | — / — | 14.8k / 16.2k | 5 / 4 | 18s / 20s | $0.071 / $0.057 | 0.99 / 2.23 |
| L09 | local-semantic | 8.0 / 10.0 | 173.6k / 52.3k | — / — | 58.1k / 28.0k | 15 / 4 | 35s / 21s | $0.150 / $0.078 | 0.46 / 1.91 |
| L10 | local-semantic | 9.0 / 9.0 | 106.7k / 22.2k | — / — | 24.3k / 7.6k | 4 / 2 | 19s / 16s | $0.084 / $0.053 | 0.84 / 4.06 |
| L11 | local-trace | 7.5 / 9.0 | 80.9k / 33.1k | — / — | 14.9k / 13.6k | 5 / 3 | 20s / 16s | $0.073 / $0.057 | 0.93 / 2.72 |
| L12 | local-semantic | 9.0 / 9.0 | 86.2k / 22.0k | — / — | 20.2k / 7.4k | 4 / 2 | 20s / 14s | $0.088 / $0.046 | 1.04 / 4.09 |
| L13 | local-trace | 8.0 / 7.5 | 59.1k / 31.0k | — / — | 9.6k / 11.6k | 4 / 3 | 17s / 18s | $0.069 / $0.056 | 1.35 / 2.42 |
| L14 | local-semantic | 8.0 / 8.5 | 53.7k / 24.8k | — / — | 4.2k / 5.3k | 2 / 3 | 14s / 17s | $0.046 / $0.042 | 1.49 / 3.43 |
| L15 | local-semantic | 9.0 / 10.0 | 59.2k / 23.3k | — / — | 9.8k / 8.8k | 2 / 2 | 16s / 17s | $0.064 / $0.053 | 1.52 / 4.29 |
| L16 | local-trace | 9.0 / 9.5 | 75.7k / 58.3k | — / — | 9.7k / 24.1k | 5 / 6 | 19s / 22s | $0.061 / $0.071 | 1.19 / 1.63 |
| L17 | local-trace | 8.5 / 8.0 | 58.3k / 29.3k | — / — | 8.9k / 9.7k | 5 / 3 | 17s / 16s | $0.062 / $0.050 | 1.46 / 2.74 |
| L18 | local-semantic | 8.5 / 10.0 | 86.8k / 53.9k | — / — | 20.9k / 29.7k | 5 / 4 | 19s / 19s | $0.094 / $0.076 | 0.98 / 1.85 |
| L19 | local-trace | 9.0 / 9.5 | 96.5k / 34.4k | — / — | 14.0k / 14.8k | 5 / 3 | 18s / 16s | $0.077 / $0.057 | 0.93 / 2.77 |
| L20 | local-impact | 9.0 / 10.0 | 86.4k / 22.8k | — / — | 20.4k / 8.2k | 4 / 2 | 20s / 14s | $0.078 / $0.047 | 1.04 / 4.38 |

## Tool usage

- **octocode**: localFetch 52, localSearch 44, ghGetHistoryItem 14, ghGetFileContent 11, ghSearchHistory 2, structureSearch 2 · counters: clasify calls 0, matchString queries 9, lspSearch calls 0, astSearch calls 0 · tool errors 3 · permission denials 0
- **rg-gh**: Bash:invocation 83 · tool errors 0 · permission denials 0

Per-question counters (non-zero):

- G01: octocode.matchString queries=2
- G05: octocode.matchString queries=3
- G06: octocode.matchString queries=2
- G07: octocode.matchString queries=1
- G10: octocode.matchString queries=1

## Judge agreement

30 question-pairs judged; 0 needed a tie-break; 0 grader errors. Order-swap spread per worker score: mean 0.31, max 2; 57/60 within 1 point, 60/60 within 2. Preferred answer consistent across both orders: 24/30.

Reference issues raised by the judge:

- G04: _check_name_is_usable is at core.py:2485-2520 at this SHA, about 3 lines later than the reference states (within tolerance).
- G10: Fact 2 omits further tcp_socket.rs Miri ignores: keepalive, reuseaddr, reuseport, send_buffer_size and recv_buffer_size socket options.
- G10: The reference omits the additional tcp_socket.rs Miri ignores for keepalive, reuseaddr, reuseport and send/receive buffer size, which are present in the diff.
- L13: Minor: keyIsExpired also returns 0 when server.allow_access_expired is set, and expireIfNeeded has a KEY_TRIMMED path for slot-trim jobs. The reference mentions neither.

## Wrong claims flagged

- G01 octocode: Metrics not obviously gated by legacy_otel (hedged; they are gated) | Hedged claim that metrics are not gated by legacy_otel — source gates metering with 'not legacy_otel'
- G01 rg-gh: Every app pays per-request overhead (span, metrics, four operation spans): ignores that enabled() bypasses when no providers are configured | Initialization failures are caught and logged: they actually send lifespan.startup.failed and re-raise | Every app pays per-request overhead (span, metrics, operation spans) — actually inert when no providers are configured (enabled() false)
- G03 rg-gh: States clientTtl eviction path only spliced the client out of kClients (leaving drain listener active); in fact kRemoveClient already closed it via client.close(() => {}) — the TTL issue was only that | clientTtl eviction 'only spliced the client out of kClients' - the old kRemoveClient already called client.close(() => {})
- G10 octocode: Loosely groups io_copy_bidirectional.rs/rt_threaded.rs as 'TCP test files' | Says io_copy_bidirectional.rs and rt_threaded.rs lose their ignores, when some are kept or newly added (misleading)
- L03 rg-gh: Callbacks registered while hooks run 'go into the new list' is misleading: outside a new atomic block they execute immediately, since autocommit is on and in_atomic_block is False | Callbacks registered while hooks are running go into the new list (actually they run immediately, since autocommit is already True and not in atomic block)
- L04 octocode: Lists self/cls stripping as part of filter construction without noting it doesn't apply on this path (minor) | Lists self/cls stripping as part of the exclusion step without noting it is skipped when from_function passes filter_args
- L04 rg-gh: Lists self/cls stripping as part of filter construction without noting it doesn't apply when filter_args is passed (minor) | Garbled citation utils/pydantic.py:~284-~280 for _create_subset_model_v2 (actually 234-276) | Lists self/cls in the filter list without noting it does not apply when filter_args is provided (always, on this path)
- L05 rg-gh: Implies add_ai_message_chunks supports sum() patterns (sum uses pairwise __add__) — minor | Minor: implies sum() uses the N-ary add_ai_message_chunks path; sum is pairwise and would still work
- L09 octocode: Attributes line 364 call to scrapePool.reload and implies any reload suppresses markers; actually in restartLoops and only when reuseCache and the old cache exists (flagged as uncertain) | Attributes the disableEndOfRunStalenessMarkers call at line 364 to reload generally; it is in restartLoops and only fires when the cache is reused (flagged as uncertain)
- L10 octocode: Window bounds cited at 470-471 (actual 474-475); approximate citations like ~587 for single-sample return are off by ~15 lines | Approximate citations somewhat off (~587 for single-sample return, actual 572; ~628-640 for scaling, actual 619-627; histogramRate ~659, actual ~640)
- L10 rg-gh: Many citations presented as exact are off by ~7-15 lines, e.g. single-sample return cited :588-590 (actual 572-574), resets :521-522 (actual 531-532), mixed warning :498-500 (actual 484-486) | Many body line citations are off by ~8-16 lines (e.g. single-sample return cited :588-590, actual 572-574; reset add cited :521-522, actual 531-532; scaling cited :608-617, actual 619-627)
- L11 octocode: Says a panic in the future is stored via panic_result_to_join_error at harness.rs:508; that is the cancel_task path, and poll panics use panic_to_error at 546 | can_read_output located at harness.rs:~395; it is at 422 | Panic stored via harness.rs:508 — that line is cancel_task; poll panics are mapped at 546/stored at 551
- L11 rg-gh: Cites harness.rs:508 (cancel_task) for storing a poll panic; the poll-panic mapping is at harness.rs:546 | Panic stored via harness.rs:508 — that is cancel_task, not the poll-panic path
- L13 rg-gh: Deletion propagates a DEL (it is UNLINK when lazyfree-lazy-expire is enabled) | Line cites inside expireIfNeeded are off by ~7-8 lines (e.g. replica check cited 3037, actually 3045; deletion cited 3056-3065, actually 3063-3071)
- L14 rg-gh: Claims 'shouldInvoke true with a timer running can only happen with maxing' — overstated; the branch is gated by maxing but the condition itself can occur otherwise
- L17 octocode: Says each property's presence ends up as None or Null, omitting Value | Puts SetPropertyPresence around line 2722 (actual 2726, minor)
- L17 rg-gh: Describes the :2065 check as 'the path with a pre-existing object'; it is actually the creator/constructor path (ResolvePropertyAndCreatorValues) | Describes the :2065 presence check as the path 'with a pre-existing object'; it is the creator/constructor-parameter path
- L20 octocode: util.go needResolveJsonModule described as choosing the message when a JSON import 'can't be resolved' (it's for imports resolved to .json with the option off) | Parser described as 'only reads' the raw value (it writes it) | Describes needResolveJsonModule as the message for when a JSON import 'can't be resolved'; it actually applies when the module resolved to .json but resolveJsonModule is off (minor)

## Cost

Worker research $3.96 (octocode $2.25, rg-gh $1.71) · judge $7.41 · reflections $3.20 · probes $0.10 · reflection synthesis $0.00 · reported Claude total $14.67. Classification provider cost and total system cost remain unknown.

## Isolation

Every run passed its profile isolation check (offered/called tools, MCP servers, no skills, no benchmark-file access).

## Accounting and coverage notes

Independent reconciliation passes all 60 workers/reflections, four probes, 60 primary judge attempts and 71 full input-freeze checks. [Exact per-question metrics](PER_QUESTION.csv) retain each worker separately. The physical experimental run remains `/private/tmp/octocode-production-fix-snapshot-20261001/packages/octocode-benchmark/compare/unified/results/production-review-fixed-v3-20261001`; this directory contains the published report copies, not resumable raw sessions.

The wins/ties/losses table uses a ±0.5-point deadband; exact score comparisons are six Octocode wins, six equal scores and eighteen losses. Explicit SDK tool errors number three; native sidecars show four row errors with overlap, for five distinct failed calls and zero transport errors. “Requests” means distinct assistant message IDs, not measured HTTP requests. Upstream HTTP counts were not persisted. Only six of thirteen offered MCP tools were used, with no AST/LSP/clasify/artifact calls; their dedicated functional acceptance is separate. Provider telemetry was absent, rather than observed numeric zero usage or billing.

All worker cache writes have one-hour TTL; weighted tokens lack a frozen tariff. Context/research decomposition is estimated. Reported Claude totals exclude earlier failed attempts, smoke/validation runs and review-agent usage; they are not independently verified invoices or total system cost. Both-order preference is consistent on 24/30 pairs; the judge is uncalibrated and no independent production threshold was agreed.

