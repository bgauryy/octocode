# Unified benchmark — run `s12-p3`

Workers: `octocode`, `rg-gh` · model claude-sonnet-5-5 · 49 questions × 1 pass · judge claude-opus-5-5 (blinded X/Y, both orders, tie-break when spread > 2).
Build: MCP dist sha256 `8a6c5f5213df` · Claude Code 2.1.288 (Claude Code).

Quality 0–10 = correctness 0–5 + completeness 0–3 + evidence 0–2. Tokens: total = all input kinds + output; research is an estimate: total − (first-request context × requests). Weighted tokens are input-token equivalents under the frozen tariff claude-sonnet-5-5@tri-20261002 ($2/M input, $4/M 1h cache write, $0.2/M cache read, $10/M output); "cold" writes each session's first cached prefix instead of reading it. Q/$ = summed quality per dollar. Cost and tokens cover the worker Claude session; classifier provider totals are separate/unknown. Efficiency = quality per 10k total tokens; weighted efficiency = quality per 10k weighted tokens.

## Totals

| worker | mean quality | median quality | total tokens | weighted tokens (warm / cold) | research tokens | output tokens | requests | tool calls | cost | time | efficiency | weighted efficiency (warm / cold) | Q/$ (warm / cold) |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| octocode | 9.09 | 9.0 | 2267.4k | 1482.3k / 2130.4k | 628.8k | 75.0k | 166 | 154 | $2.96 | 12.8 min | 1.96 | 3.01 / 2.09 | 150.3 / 104.6 |
| rg-gh | 9.11 | 9.0 | 1488.9k | 1272.8k / 1448.5k | 531.7k | 78.6k | 198 | 156 | $2.55 | 14.5 min | 3.00 | 3.51 / 3.08 | 175.4 / 154.1 |

Per-question ratio octocode/rg-gh: total tokens mean 1.58× / median 1.49×; weighted tokens mean 1.17× / median 1.09×; research tokens mean 1.28× / median 1.05×; cost mean 1.17× / median 1.09×. Quality delta (octocode − rg-gh): mean -0.02, wins/ties/losses 9/28/12.

## Fixed overhead ("reply OK" probe)

| worker | first-request context | tools offered | isolation probe |
|---|--:|--:|---|
| octocode | — | — | not run |
| rg-gh | — | — | not run |

## By category

| category | n | octocode quality | octocode tokens | octocode research | rg-gh quality | rg-gh tokens | rg-gh research |
|---|--:|--:|--:|--:|--:|--:|--:|
| github | 18 | 9.00 | 43.9k | 12.8k | 9.06 | 27.1k | 10.4k |
| local | 31 | 9.15 | 47.7k | 12.8k | 9.15 | 32.3k | 11.1k |
| artifact | 2 | 10.00 | 39.8k | 5.4k | 10.00 | 20.7k | 6.3k |
| github-bug-rca | 2 | 8.75 | 38.2k | 8.8k | 9.25 | 23.6k | 9.3k |
| github-code-research | 3 | 8.83 | 56.3k | 17.1k | 9.50 | 31.2k | 12.2k |
| github-history | 2 | 9.75 | 36.3k | 6.8k | 8.50 | 14.1k | 2.1k |
| github-pr-review | 5 | 8.20 | 51.9k | 24.5k | 8.30 | 39.4k | 20.3k |
| github-repo-discovery | 2 | 10.00 | 38.7k | 4.3k | 10.00 | 19.7k | 3.0k |
| github-structure | 2 | 8.75 | 27.8k | 3.3k | 8.75 | 20.7k | 4.0k |
| local-described-target | 2 | 9.25 | 38.3k | 8.5k | 10.00 | 25.7k | 6.2k |
| local-impact | 2 | 9.00 | 37.4k | 7.7k | 9.25 | 29.7k | 7.8k |
| local-locate | 1 | 9.00 | 45.8k | 6.3k | 8.50 | 23.4k | 4.0k |
| local-semantic | 6 | 9.67 | 53.9k | 17.6k | 8.92 | 32.5k | 12.3k |
| local-structure | 2 | 9.25 | 52.3k | 12.8k | 8.50 | 34.0k | 9.7k |
| local-symbol | 2 | 9.00 | 29.3k | 4.5k | 9.00 | 22.2k | 5.1k |
| local-trace | 11 | 8.86 | 54.7k | 16.0k | 9.05 | 35.5k | 13.4k |
| mixed | 5 | 9.20 | 38.4k | 8.6k | 9.70 | 33.8k | 12.3k |

Token columns are per-question means.

## Per question

Each cell shows octocode / rg-gh.

| q | category | quality | total tokens | weighted tokens | research tokens | tool calls | time | cost | efficiency |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|
| G01 | github-pr-review | 9.5 / 8.0 | 89.4k / 68.7k | 75.7k / 58.7k | 50.2k / 40.1k | 3 / 7 | 33s / 39s | $0.151 / $0.117 | 1.06 / 1.16 |
| G02 | github-pr-review | 8.0 / 8.0 | 53.6k / 45.2k | 43.9k / 42.3k | 24.2k / 26.2k | 2 / 3 | 14s / 24s | $0.088 / $0.085 | 1.49 / 1.77 |
| G03 | github-pr-review | 8.0 / 9.0 | 28.5k / 18.1k | 29.1k / 27.3k | 8.9k / 8.5k | 1 / 1 | 12s / 14s | $0.058 / $0.055 | 2.80 / 4.98 |
| G04 | github-pr-review | 8.5 / 7.5 | 29.3k / 27.6k | 30.2k / 24.3k | 9.7k / 8.6k | 1 / 3 | 11s / 15s | $0.060 / $0.049 | 2.90 / 2.72 |
| G05 | github-code-research | 9.5 / 9.5 | 56.5k / 40.0k | 36.1k / 28.6k | 17.3k / 16.3k | 3 / 4 | 17s / 21s | $0.072 / $0.057 | 1.68 / 2.37 |
| G06 | github-code-research | 7.0 / 9.0 | 47.9k / 27.1k | 26.1k / 24.4k | 8.6k / 8.1k | 5 / 4 | 19s / 18s | $0.052 / $0.049 | 1.46 / 3.32 |
| G07 | github-code-research | 10.0 / 10.0 | 64.7k / 26.4k | 44.3k / 32.8k | 25.5k / 12.1k | 6 / 3 | 27s / 18s | $0.089 / $0.066 | 1.55 / 3.78 |
| G08 | github-bug-rca | 8.5 / 9.5 | 34.4k / 19.4k | 19.2k / 18.0k | 5.0k / 5.2k | 2 / 2 | 10s / 12s | $0.038 / $0.036 | 2.47 / 4.89 |
| G09 | github-bug-rca | 9.0 / 9.0 | 42.0k / 27.7k | 33.6k / 35.2k | 12.6k / 13.5k | 2 / 2 | 14s / 20s | $0.067 / $0.070 | 2.15 / 3.25 |
| G10 | github-pr-review | 7.0 / 9.0 | 58.7k / 37.2k | 52.8k / 36.0k | 29.2k / 18.1k | 2 / 3 | 20s / 22s | $0.106 / $0.072 | 1.19 / 2.42 |
| L01 | local-trace | 9.0 / 9.0 | 48.1k / 25.5k | 22.9k / 20.2k | 8.6k / 6.2k | 3 / 3 | 14s / 17s | $0.046 / $0.040 | 1.87 / 3.53 |
| L02 | local-locate | 9.0 / 8.5 | 45.8k / 23.4k | 21.0k / 17.0k | 6.3k / 4.0k | 3 / 3 | 15s / 15s | $0.042 / $0.034 | 1.96 / 3.64 |
| L03 | local-trace | 10.0 / 9.0 | 56.5k / 35.4k | 35.1k / 26.0k | 17.0k / 11.2k | 5 / 4 | 16s / 17s | $0.070 / $0.052 | 1.77 / 2.54 |
| L04 | local-trace | 8.0 / 8.0 | 111.6k / 59.9k | 66.2k / 43.8k | 52.1k / 30.6k | 10 / 5 | 28s / 25s | $0.132 / $0.088 | 0.72 / 1.34 |
| L05 | local-impact | 8.0 / 9.0 | 23.8k / 25.1k | 18.2k / 19.7k | 4.0k / 5.6k | 1 / 3 | 11s / 13s | $0.036 / $0.039 | 3.36 / 3.58 |
| L06 | local-trace | 9.0 / 9.0 | 53.7k / 36.0k | 27.2k / 25.6k | 14.2k / 11.8k | 3 / 4 | 16s / 19s | $0.054 / $0.051 | 1.68 / 2.50 |
| L07 | local-trace | 9.0 / 10.0 | 45.0k / 32.6k | 35.8k / 29.7k | 15.3k / 13.1k | 5 / 3 | 18s / 18s | $0.072 / $0.059 | 2.00 / 3.07 |
| L08 | local-trace | 9.0 / 9.0 | 38.4k / 29.0k | 26.5k / 26.7k | 8.8k / 9.7k | 3 / 4 | 14s / 17s | $0.053 / $0.053 | 2.34 / 3.10 |
| L09 | local-semantic | 10.0 / 9.0 | 78.1k / 41.4k | 46.1k / 35.6k | 28.6k / 17.1k | 6 / 4 | 24s / 23s | $0.092 / $0.071 | 1.28 / 2.18 |
| L10 | local-semantic | 10.0 / 9.0 | 42.5k / 27.4k | 37.4k / 26.1k | 12.8k / 8.0k | 2 / 3 | 16s / 16s | $0.075 / $0.052 | 2.35 / 3.29 |
| L11 | local-trace | 10.0 / 9.5 | 60.9k / 30.9k | 40.1k / 32.3k | 21.3k / 11.5k | 3 / 3 | 20s / 20s | $0.080 / $0.065 | 1.64 / 3.08 |
| L12 | local-semantic | 9.5 / 9.0 | 43.9k / 28.5k | 30.8k / 26.8k | 14.1k / 9.0k | 2 / 3 | 14s / 21s | $0.062 / $0.054 | 2.17 / 3.16 |
| L13 | local-trace | 7.5 / 9.0 | 50.6k / 35.9k | 32.5k / 31.1k | 11.1k / 16.5k | 4 / 3 | 16s / 15s | $0.065 / $0.062 | 1.48 / 2.50 |
| L14 | local-semantic | 9.0 / 7.5 | 34.1k / 24.4k | 20.7k / 19.5k | 4.4k / 5.0k | 2 / 3 | 12s / 19s | $0.041 / $0.039 | 2.64 / 3.07 |
| L15 | local-semantic | 9.5 / 9.0 | 55.8k / 28.8k | 33.3k / 27.5k | 16.2k / 9.5k | 3 / 3 | 15s / 18s | $0.067 / $0.055 | 1.70 / 3.12 |
| L16 | local-trace | 8.0 / 8.0 | 37.8k / 29.1k | 27.2k / 25.0k | 8.1k / 9.6k | 3 / 3 | 16s / 18s | $0.054 / $0.050 | 2.12 / 2.75 |
| L17 | local-trace | 9.0 / 9.0 | 35.9k / 36.1k | 23.5k / 27.0k | 6.2k / 11.8k | 2 / 4 | 14s / 19s | $0.047 / $0.054 | 2.50 / 2.49 |
| L18 | local-semantic | 10.0 / 10.0 | 68.9k / 44.7k | 54.5k / 40.3k | 29.3k / 25.3k | 4 / 3 | 24s / 22s | $0.109 / $0.081 | 1.45 / 2.24 |
| L19 | local-trace | 9.0 / 10.0 | 63.1k / 39.7k | 34.9k / 30.4k | 13.6k / 15.3k | 6 / 4 | 18s / 21s | $0.070 / $0.061 | 1.43 / 2.52 |
| L20 | local-impact | 10.0 / 9.5 | 51.0k / 34.2k | 27.5k / 24.8k | 11.4k / 9.9k | 4 / 4 | 17s / 20s | $0.055 / $0.050 | 1.96 / 2.78 |
| G11 | github-structure | 9.0 / 8.5 | 21.8k / 17.0k | 14.3k / 14.8k | 2.1k / 2.6k | 2 / 2 | 11s / 16s | $0.029 / $0.030 | 4.14 / 5.00 |
| G12 | github-structure | 8.5 / 9.0 | 33.9k / 24.4k | 21.4k / 21.3k | 4.4k / 5.3k | 3 / 3 | 15s / 16s | $0.043 / $0.043 | 2.51 / 3.69 |
| G13 | github-repo-discovery | 10.0 / 10.0 | 44.8k / 16.5k | 20.2k / 12.8k | 5.5k / 2.1k | 4 / 2 | 16s / 9s | $0.040 / $0.026 | 2.23 / 6.06 |
| G14 | github-repo-discovery | 10.0 / 10.0 | 32.5k / 23.0k | 16.0k / 16.0k | 3.1k / 3.9k | 2 / 3 | 11s / 12s | $0.032 / $0.032 | 3.07 / 4.35 |
| G15 | github-history | 9.5 / 7.0 | 37.7k / 11.0k | 24.1k / 11.4k | 8.2k / 1.4k | 3 / 1 | 17s / 9s | $0.048 / $0.023 | 2.52 / 6.38 |
| G16 | github-history | 10.0 / 10.0 | 34.9k / 17.3k | 20.2k / 14.8k | 5.4k / 2.9k | 4 / 2 | 13s / 13s | $0.040 / $0.030 | 2.87 / 5.78 |
| G17 | artifact | 10.0 / 10.0 | 33.4k / 14.2k | 19.8k / 19.2k | 4.0k / 4.6k | 2 / 1 | 13s / 13s | $0.040 / $0.038 | 2.99 / 7.04 |
| G18 | artifact | 10.0 / 10.0 | 46.1k / 27.1k | 22.5k / 22.4k | 6.8k / 7.9k | 4 / 4 | 17s / 19s | $0.045 / $0.045 | 2.17 / 3.69 |
| L21 | local-symbol | 9.0 / 8.0 | 37.2k / 27.2k | 24.4k / 24.7k | 7.4k / 7.7k | 2 / 3 | 14s / 17s | $0.049 / $0.049 | 2.42 / 2.94 |
| L22 | local-symbol | 9.0 / 10.0 | 21.4k / 17.2k | 12.5k / 14.6k | 1.6k / 2.5k | 1 / 2 | 7s / 13s | $0.025 / $0.029 | 4.20 / 5.83 |
| L23 | local-structure | 9.0 / 8.0 | 49.1k / 23.3k | 28.8k / 16.3k | 9.6k / 4.0k | 4 / 3 | 16s / 12s | $0.058 / $0.033 | 1.83 / 3.43 |
| L24 | local-structure | 9.5 / 9.0 | 55.5k / 44.6k | 34.6k / 31.8k | 16.0k / 15.5k | 4 / 5 | 19s / 23s | $0.069 / $0.064 | 1.71 / 2.02 |
| L25 | local-described-target | 9.5 / 10.0 | 41.8k / 25.0k | 28.7k / 19.7k | 12.0k / 5.4k | 2 / 3 | 15s / 18s | $0.057 / $0.039 | 2.27 / 4.01 |
| L26 | local-described-target | 9.0 / 10.0 | 34.8k / 26.5k | 21.4k / 21.8k | 5.0k / 7.0k | 2 / 3 | 11s / 15s | $0.043 / $0.044 | 2.59 / 3.77 |
| L27 | mixed | 9.0 / 9.5 | 49.9k / 35.7k | 25.6k / 26.4k | 10.1k / 11.1k | 4 / 4 | 16s / 18s | $0.051 / $0.053 | 1.80 / 2.66 |
| L28 | mixed | 10.0 / 10.0 | 40.0k / 39.0k | 28.8k / 29.7k | 10.2k / 14.6k | 4 / 4 | 14s / 24s | $0.058 / $0.059 | 2.50 / 2.56 |
| L29 | mixed | 8.0 / 9.0 | 38.5k / 38.8k | 25.6k / 28.6k | 8.7k / 14.2k | 3 / 4 | 12s / 19s | $0.051 / $0.057 | 2.08 / 2.32 |
| L30 | mixed | 10.0 / 10.0 | 41.5k / 37.3k | 26.9k / 29.2k | 11.7k / 17.7k | 2 / 4 | 12s / 17s | $0.054 / $0.058 | 2.41 / 2.68 |
| L31 | mixed | 9.0 / 10.0 | 22.0k / 18.4k | 14.0k / 14.6k | 2.1k / 3.7k | 1 / 2 | 8s / 10s | $0.028 / $0.029 | 4.08 / 5.44 |

## Tool usage

- **octocode**: localSearch 51, localFetch 43, ghGetHistoryItem 19, ghGetFileContent 19, ghSearchCode 10, ghSearchHistory 4, structureSearch 2, ghStructure 2, artifactSearch 2, astSearch 1, lspSearch 1 · counters: clasify calls 0, matchString queries 16, lspSearch calls 1, astSearch calls 1 · tool errors 4 · permission denials 0
- **rg-gh**: Bash:invocation 156 · tool errors 15 · permission denials 0

Per-question counters (non-zero):

- G01: octocode.matchString queries=2
- G05: octocode.matchString queries=2
- G06: octocode.matchString queries=1
- G07: octocode.matchString queries=3
- G10: octocode.matchString queries=1
- L17: octocode.matchString queries=1
- G13: octocode.matchString queries=2
- G14: octocode.matchString queries=1
- G17: octocode.matchString queries=2
- G18: octocode.matchString queries=1
- L23: octocode.lspSearch calls=1, octocode.astSearch calls=1

## Judge agreement

49 question-pairs judged; 0 needed a tie-break; 0 grader errors. Order-swap spread per worker score: mean 0.24, max 2; 96/98 within 1 point, 98/98 within 2. Preferred answer consistent across both orders: 40/49.

Reference issues raised by the judge:

- G10: Minor: the readv leak is linked as miri pull/5054 in the code, not an issue. Otherwise accurate.
- G10: Minor: the source links rust-lang/miri#5054 as a pull (miri/pull/5054), not an issue. Otherwise consistent with the diff.
- L13: Reference omits the slot-trim KEY_TRIMMED branch and the cluster_enabled CLIENT_MASTER condition in expireIfNeeded (db.c:3011-3018, 3043-3044), and server.allow_access_expired in keyIsExpired; minor.
- L14: Minor: the maxWait assignment at 10422 is conditional on maxing (`maxing ? nativeMax(...) : maxWait`); otherwise accurate.
- L20: The reference leaves out the showconfig.go:47 computed-default entry as a consumer. Its notes only mention showconfig in passing. Both answers correctly include it.
- L20: The reference omits tsoptions/showconfig.go:47, which uses GetResolveJsonModule via a method expression (computeFn((*core.CompilerOptions).GetResolveJsonModule)) as a computed showConfig default. Its rg pattern with '\(' misses it, so the claim of exactly 4 non-test call sites is incomplete.
- G16: The reference names the label as `__unix_socket__` (UnixSocketLabel), but the PR #19399 body uses `__scrape_unix_socket__`. I could not check the code at ea954809ce, so the reference's label name may be wrong.
- G16: The reference names the label `__unix_socket__`, but PR #19399's body uses `__scrape_unix_socket__`, as both answers do. I could not check scrape.go at ea954809ce. The label name in the reference may be inaccurate, but it is not graded.
- G17: The reference omits the empty 'security' extra declared in extras_require (setup.py:124). install_requires spans lines 61-66 (the list's closing bracket is on 66).
- G17: Reference omits the empty 'security' extra declared at setup.py:124 (minor).
- L24: The reference omits Manager.Stop (manager.go:346-353), which starts goroutines via errgroup.Group.Go (limit GOMAXPROCS), each calling sp.stop(). So seven functions start goroutines, not six; the reference's count holds only for literal `go` statements.
- L24: Reference misses Manager.Stop (manager.go:346-353), which starts goroutines via errgroup.Group.Go with SetLimit(GOMAXPROCS), each calling sp.stop(). 'Exactly six functions' holds only for literal go statements.
- L30: Reference omits the wraparound_unbounded test that the PR also adds; otherwise accurate.

## Wrong claims flagged

- G01 rg-gh: Unsupported OTEL_*_EXPORTER values only warn (_runtime.py:53); the code actually raises FastAPIError, which fails lifespan startup | Says telemetry is 'on unless you opt out' and omits the enabled() provider condition (inert when no provider is configured) | Unsupported OTEL_*_EXPORTER values only warn (_runtime.py:53) — the code actually raises FastAPIError, which fails startup
- G03 octocode: Pool only spliced the client out on clientTtl eviction (it actually called kRemoveClient, which closed it) | Implies the TTL path was changed to call kRetireClient instead of splicing (the TTL path is unchanged; kRemoveClient now delegates) | Claims Pool only spliced clients out on clientTtl eviction; it actually used kRemoveClient, which closed the client but did not track it
- G04 octocode: click.option("--0-file") now warns (exposed option with non-identifier derived name still raises TypeError; only unexposed warns)
- G04 rg-gh: Muddled self-contradiction about current Foo_Bar behavior and claim upgrade guide example reads the other way (final statement correct) | click.option("--0-file") warns (it raises TypeError when exposed) | Muddled explanation of current casing behavior, ending in 'ambiguous'
- G06 octocode: Speculates get_environment_proxies 'probably lives elsewhere' than _utils.py; it is defined at httpx/_utils.py:30 (hedged) | Suggests get_environment_proxies is not in httpx/_utils.py (it is, at line 30); hedged
- G10 octocode: Its list of enabled TCP tests leaves out most tcp_* files and lists io_copy_bidirectional, where only some tests were enabled | Loosely labels io_driver/rt_common etc. as 'TCP tests' while omitting most tcp_*.rs files
- L03 rg-gh: Nested on_commit during hook run 'adds to the new list' — actually runs immediately since autocommit is already True and no atomic block (partly self-corrected)
- L04 octocode: Implies self/cls are added to the exclusions on this path (only happens when filter_args is empty) | Claims self/cls stripping is applied on this path (it is skipped because from_function always passes a non-empty filter_args)
- L04 rg-gh: Lists self/cls dropping for methods as applying on this path; @tool->from_function always passes non-empty filter_args so base.py:324-329 is skipped | Claims self/cls are dropped for methods on this path | Says Returns:/Example: blocks are skipped, when every block after them (until Args:) is skipped too
- L05 octocode: Says merge_content's output in add_ai_message_chunks is used for the merged additional_kwargs; additional_kwargs is computed independently via merge_dicts (ai.py:666-668) | merge_content output at ai.py:665 is 'also used for the AI chunk's merged additional_kwargs' — additional_kwargs is computed independently via merge_dicts
- L12 rg-gh: Says the lifo slot task is moved to the run queue when a worker shuts down (worker.rs:479-481); that code is actually the block_in_place core handoff | Says worker.rs:479-481 moves the slot task to the run queue 'when a worker shuts down'; that code is the block_in_place core handoff, not shutdown
- L13 octocode: Main answer states 'a DEL is propagated' (could be UNLINK with lazyfree-lazy-expire), though later hedged | Summary says 'a DEL is propagated' (UNLINK when lazyfree-lazy-expire is on; hedged later) | Garbled description of the confAllowsExpireDel condition
- L14 rg-gh: States debounced records lastArgs/lastThis/lastCallTime and then checks shouldInvoke; the code computes shouldInvoke first (10501)
- L16 octocode: Claims the code's *20L check gives a minimum of ~20 per segment, contradicting the '10' comment (the comment is correct) | Claims the code's *20L check gives a minimum of about 20 entries per segment, contradicting the comment; it actually gives about 10 (e.g. maxWeight=20 yields 2 segments of 10)
- L16 rg-gh: Each segment keeps at least about 20 units of weight (actually ~10 after the final doubling) | 'Each segment keeps at least about 20 units of weight': the guarantee is about 10
- L20 rg-gh: TODO cited at :274 (actually :275) — trivial | Describes needResolveJsonModule as choosing message when JSON import 'can't be resolved' — it's when resolved to .json with option off
- G11 rg-gh: Opening summary says two transports work with both clients, two sync-only and one async-only; the correct split is 1 both, 2 sync-only, 2 async-only (its own table says so).
- G12 rg-gh: States __init__.py exports 14 names; it actually re-exports 15 (its own list has 15) | Says __init__.py exports 14 names; it exports 15, and the list it gives contains all 15
- G15 rg-gh: #8067 described as an open PR; it is an open issue that #8337 refs, not something #8337 replaced | Calls #8067 a PR; it is an open issue
- L21 rg-gh: Names the locked variant's method LockedInner::begin_shutdown (actually LockedImpl; LockedInner is the mutex contents) | Names the sharded type 'Sharded' rather than ShardedImpl | Calls the Locked method LockedInner::begin_shutdown; it is LockedImpl::begin_shutdown (LockedInner is the mutex contents)
- L22 octocode: Line 121 described as 'in the Media code' — it is MediaAsset.path (minor)
- L23 rg-gh: States the 24 call sites sit in 24 QuerySet methods; there are 23 methods (its own list has 23) | Says the 24 call sites sit in 24 QuerySet methods; there are 23, and its own list has 23 entries.
- L24 octocode: Claims exactly six functions start goroutines; Manager.Stop also starts them via errgroup.Go
- L24 rg-gh: Attributes the stop/start goroutines at scrape.go:554/582 to scrapePool.Sync; they are in the unexported scrapePool.sync (scrape.go:485) | Attributes the two goroutines to scrapePool.Sync; they are in the lowercase scrapePool.sync (scrape.go:485)
- L25 octocode: Mismatched-cycle example ('a refers to itself, b refers to a different object' => false) is overgeneralized; if the other object is itself a structurally equal self-cycle, the result can be true
- L29 octocode: Implies Acquire::drop is a subsequent operation that dereferences the dangling waiter; with queued=true drop unlinks safely, and the actual UAF path (poll completes -> queued=false -> drop skips unlin | Vaguely says Acquire::drop 'sees a completed waiter' as the next step, without the queued-flag mechanism; a drop with queued=true would actually unlink correctly
- L30 octocode: Labels channel_from_list and list::channel_from_index as test-only additions (minor) | Lists channel_from_list under 'Test-only additions' though it is a non-test helper (minor)
- L30 rg-gh: Groups list::channel_from_index and chan::channel_from_list under 'tests only' though they are not cfg(test)-gated (minor)

## Cost

Worker research $5.51 (octocode $2.96, rg-gh $2.55) · judge $11.42 · reflections $4.80 · probes $0.00 · reflection synthesis $0.00 · reported Claude total $21.73. Classification provider cost and total system cost remain unknown.

## Isolation

Every run passed its profile isolation check (offered/called tools, MCP servers, no skills, no benchmark-file access).
