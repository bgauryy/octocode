# Unified benchmark — run `s12-p1`

Workers: `octocode`, `rg-gh` · model claude-sonnet-5-5 · 49 questions × 1 pass · judge claude-opus-5-5 (blinded X/Y, both orders, tie-break when spread > 2).
Build: MCP dist sha256 `8a6c5f5213df` · Claude Code 2.1.288 (Claude Code).

Quality 0–10 = correctness 0–5 + completeness 0–3 + evidence 0–2. Tokens: total = all input kinds + output; research is an estimate: total − (first-request context × requests). Weighted tokens are input-token equivalents under the frozen tariff claude-sonnet-5-5@tri-20261002 ($2/M input, $4/M 1h cache write, $0.2/M cache read, $10/M output); "cold" writes each session's first cached prefix instead of reading it. Q/$ = summed quality per dollar. Cost and tokens cover the worker Claude session; classifier provider totals are separate/unknown. Efficiency = quality per 10k total tokens; weighted efficiency = quality per 10k weighted tokens.

## Totals

| worker | mean quality | median quality | total tokens | weighted tokens (warm / cold) | research tokens | output tokens | requests | tool calls | cost | time | efficiency | weighted efficiency (warm / cold) | Q/$ (warm / cold) |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| octocode | 8.97 | 9.0 | 2311.7k | 1485.4k / 2133.5k | 633.2k | 76.4k | 170 | 161 | $2.97 | 12.9 min | 1.90 | 2.96 / 2.06 | 147.9 / 103.0 |
| rg-gh | 9.16 | 9.0 | 1577.4k | 1294.5k / 1470.1k | 600.5k | 78.9k | 202 | 161 | $2.59 | 15.1 min | 2.85 | 3.47 / 3.05 | 173.4 / 152.7 |

Per-question ratio octocode/rg-gh: total tokens mean 1.64× / median 1.48×; weighted tokens mean 1.19× / median 1.16×; research tokens mean 1.37× / median 1.17×; cost mean 1.19× / median 1.16×. Quality delta (octocode − rg-gh): mean -0.19, wins/ties/losses 8/28/13.

## Fixed overhead ("reply OK" probe)

| worker | first-request context | tools offered | isolation probe |
|---|--:|--:|---|
| octocode | 9.8k | 13 | pass |
| rg-gh | 4.7k | 1 | pass |

## By category

| category | n | octocode quality | octocode tokens | octocode research | rg-gh quality | rg-gh tokens | rg-gh research |
|---|--:|--:|--:|--:|--:|--:|--:|
| github | 18 | 8.92 | 40.1k | 10.6k | 9.11 | 28.5k | 12.3k |
| local | 31 | 9.00 | 51.3k | 14.3k | 9.19 | 34.3k | 12.2k |
| artifact | 2 | 10.00 | 40.6k | 6.2k | 10.00 | 20.3k | 5.9k |
| github-bug-rca | 2 | 8.75 | 38.3k | 9.0k | 8.75 | 22.3k | 8.0k |
| github-code-research | 3 | 9.00 | 46.6k | 14.0k | 9.00 | 31.7k | 12.6k |
| github-history | 2 | 9.50 | 36.6k | 7.1k | 8.75 | 14.3k | 2.3k |
| github-pr-review | 5 | 7.90 | 43.4k | 17.9k | 8.80 | 44.6k | 27.4k |
| github-repo-discovery | 2 | 10.00 | 37.7k | 3.3k | 9.50 | 26.0k | 4.4k |
| github-structure | 2 | 8.75 | 29.0k | 4.4k | 9.50 | 14.8k | 2.8k |
| local-described-target | 2 | 9.75 | 61.0k | 16.4k | 9.50 | 23.4k | 6.3k |
| local-impact | 2 | 9.25 | 46.6k | 11.9k | 9.25 | 25.5k | 6.0k |
| local-locate | 1 | 8.00 | 45.6k | 6.1k | 8.00 | 24.5k | 5.1k |
| local-semantic | 6 | 9.33 | 57.7k | 19.7k | 9.50 | 37.0k | 14.4k |
| local-structure | 2 | 7.50 | 47.1k | 12.5k | 8.00 | 28.6k | 9.2k |
| local-symbol | 2 | 8.75 | 30.2k | 5.4k | 9.25 | 25.2k | 5.7k |
| local-trace | 11 | 8.91 | 54.8k | 15.2k | 9.09 | 37.7k | 14.3k |
| mixed | 5 | 9.30 | 45.3k | 11.5k | 9.60 | 39.6k | 15.1k |

Token columns are per-question means.

## Per question

Each cell shows octocode / rg-gh.

| q | category | quality | total tokens | weighted tokens | research tokens | tool calls | time | cost | efficiency |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|
| G01 | github-pr-review | 7.0 / 9.0 | 47.4k / 118.6k | 49.2k / 78.3k | 18.0k / 90.0k | 3 / 6 | 22s / 42s | $0.098 / $0.157 | 1.48 / 0.76 |
| G02 | github-pr-review | 8.0 / 9.0 | 57.2k / 28.3k | 52.2k / 37.7k | 27.8k / 14.0k | 2 / 2 | 16s / 17s | $0.104 / $0.075 | 1.40 / 3.18 |
| G03 | github-pr-review | 9.0 / 9.0 | 28.5k / 18.0k | 28.8k / 27.2k | 8.9k / 8.5k | 1 / 1 | 12s / 13s | $0.058 / $0.054 | 3.16 / 4.99 |
| G04 | github-pr-review | 8.0 / 8.0 | 29.3k / 19.8k | 30.2k / 19.9k | 9.8k / 5.5k | 1 / 2 | 11s / 14s | $0.060 / $0.040 | 2.73 / 4.05 |
| G05 | github-code-research | 9.0 / 9.5 | 41.8k / 33.4k | 30.0k / 32.1k | 12.4k / 14.4k | 2 / 3 | 17s / 23s | $0.060 / $0.064 | 2.15 / 2.85 |
| G06 | github-code-research | 8.0 / 8.0 | 51.7k / 25.3k | 31.1k / 20.5k | 12.5k / 6.3k | 5 / 3 | 21s / 15s | $0.062 / $0.041 | 1.55 / 3.16 |
| G07 | github-code-research | 10.0 / 9.5 | 46.3k / 36.3k | 38.5k / 33.8k | 16.9k / 17.3k | 5 / 4 | 19s / 23s | $0.077 / $0.068 | 2.16 / 2.62 |
| G08 | github-bug-rca | 8.5 / 9.5 | 34.4k / 19.4k | 19.5k / 18.1k | 5.1k / 5.1k | 2 / 2 | 11s / 15s | $0.039 / $0.036 | 2.47 / 4.91 |
| G09 | github-bug-rca | 9.0 / 8.0 | 42.2k / 25.2k | 34.8k / 28.7k | 12.8k / 11.0k | 2 / 2 | 14s / 19s | $0.070 / $0.057 | 2.13 / 3.18 |
| G10 | github-pr-review | 7.5 / 9.0 | 54.7k / 38.1k | 44.9k / 36.8k | 25.2k / 19.1k | 2 / 3 | 18s / 22s | $0.090 / $0.074 | 1.37 / 2.36 |
| L01 | local-trace | 9.0 / 9.5 | 36.0k / 25.8k | 22.9k / 20.5k | 6.4k / 6.4k | 3 / 3 | 11s / 16s | $0.046 / $0.041 | 2.50 / 3.69 |
| L02 | local-locate | 8.0 / 8.0 | 45.6k / 24.5k | 22.3k / 18.2k | 6.1k / 5.1k | 4 / 3 | 15s / 18s | $0.045 / $0.036 | 1.75 / 3.27 |
| L03 | local-trace | 10.0 / 9.0 | 95.8k / 38.4k | 38.2k / 30.5k | 26.7k / 14.2k | 7 / 4 | 24s / 18s | $0.076 / $0.061 | 1.04 / 2.34 |
| L04 | local-trace | 8.0 / 8.5 | 62.6k / 71.9k | 46.9k / 46.1k | 23.0k / 37.8k | 6 / 7 | 22s / 26s | $0.094 / $0.092 | 1.28 / 1.18 |
| L05 | local-impact | 8.5 / 8.5 | 23.7k / 24.1k | 17.7k / 17.4k | 3.9k / 4.6k | 1 / 3 | 9s / 15s | $0.035 / $0.035 | 3.58 / 3.53 |
| L06 | local-trace | 9.0 / 9.0 | 51.6k / 26.0k | 27.7k / 21.2k | 12.1k / 6.7k | 3 / 3 | 17s / 16s | $0.055 / $0.042 | 1.74 / 3.46 |
| L07 | local-trace | 8.5 / 10.0 | 40.1k / 44.1k | 29.7k / 32.1k | 10.4k / 19.8k | 4 / 4 | 18s / 23s | $0.059 / $0.064 | 2.12 / 2.27 |
| L08 | local-trace | 8.0 / 9.0 | 27.0k / 35.3k | 25.6k / 25.7k | 7.3k / 11.2k | 1 / 4 | 11s / 20s | $0.051 / $0.051 | 2.96 / 2.55 |
| L09 | local-semantic | 10.0 / 10.0 | 102.9k / 46.9k | 52.4k / 39.0k | 43.5k / 22.6k | 6 / 4 | 26s / 28s | $0.105 / $0.078 | 0.97 / 2.13 |
| L10 | local-semantic | 10.0 / 9.0 | 38.7k / 27.5k | 30.4k / 26.2k | 9.1k / 8.1k | 3 / 3 | 13s / 15s | $0.061 / $0.052 | 2.58 / 3.27 |
| L11 | local-trace | 10.0 / 10.0 | 57.2k / 39.1k | 38.1k / 30.5k | 17.7k / 14.8k | 6 / 4 | 20s / 21s | $0.076 / $0.061 | 1.75 / 2.56 |
| L12 | local-semantic | 9.0 / 9.5 | 62.7k / 28.2k | 40.9k / 26.0k | 23.1k / 8.7k | 4 / 3 | 20s / 17s | $0.082 / $0.052 | 1.44 / 3.37 |
| L13 | local-trace | 8.5 / 8.0 | 77.0k / 38.9k | 41.4k / 29.5k | 27.6k / 14.6k | 4 / 4 | 19s / 19s | $0.083 / $0.059 | 1.10 / 2.06 |
| L14 | local-semantic | 9.0 / 9.0 | 34.2k / 30.8k | 21.2k / 22.1k | 4.5k / 6.5k | 2 / 4 | 15s / 23s | $0.042 / $0.044 | 2.63 / 2.92 |
| L15 | local-semantic | 8.0 / 10.0 | 42.1k / 43.8k | 32.7k / 35.6k | 12.4k / 19.6k | 3 / 4 | 16s / 22s | $0.065 / $0.071 | 1.90 / 2.28 |
| L16 | local-trace | 9.0 / 8.0 | 39.5k / 30.3k | 30.4k / 26.5k | 9.8k / 10.8k | 3 / 3 | 14s / 16s | $0.061 / $0.053 | 2.28 / 2.64 |
| L17 | local-trace | 9.0 / 10.0 | 53.3k / 36.2k | 33.6k / 27.2k | 13.7k / 11.9k | 4 / 4 | 18s / 20s | $0.067 / $0.054 | 1.69 / 2.76 |
| L18 | local-semantic | 10.0 / 9.5 | 65.4k / 44.9k | 41.3k / 33.0k | 25.9k / 20.6k | 4 / 4 | 19s / 28s | $0.083 / $0.066 | 1.53 / 2.12 |
| L19 | local-trace | 9.0 / 9.0 | 62.2k / 28.7k | 33.0k / 25.5k | 12.7k / 9.2k | 5 / 3 | 20s / 19s | $0.066 / $0.051 | 1.45 / 3.14 |
| L20 | local-impact | 10.0 / 10.0 | 69.5k / 26.8k | 38.1k / 24.2k | 20.0k / 7.4k | 4 / 3 | 18s / 19s | $0.076 / $0.048 | 1.44 / 3.73 |
| G11 | github-structure | 9.5 / 9.0 | 22.4k / 11.2k | 15.5k / 12.4k | 2.7k / 1.7k | 2 / 1 | 9s / 10s | $0.031 / $0.025 | 4.25 / 8.02 |
| G12 | github-structure | 8.0 / 10.0 | 35.6k / 18.3k | 22.2k / 19.2k | 6.1k / 4.0k | 3 / 2 | 14s / 16s | $0.044 / $0.038 | 2.25 / 5.46 |
| G13 | github-repo-discovery | 10.0 / 10.0 | 43.1k / 22.9k | 17.7k / 16.2k | 3.8k / 3.7k | 4 / 3 | 13s / 12s | $0.035 / $0.032 | 2.32 / 4.37 |
| G14 | github-repo-discovery | 10.0 / 9.0 | 32.3k / 29.1k | 15.9k / 17.3k | 2.8k / 5.1k | 2 / 4 | 11s / 13s | $0.032 / $0.035 | 3.10 / 3.10 |
| G15 | github-history | 9.0 / 7.5 | 37.1k / 10.9k | 22.3k / 11.3k | 7.6k / 1.3k | 2 / 1 | 14s / 9s | $0.045 / $0.023 | 2.43 / 6.87 |
| G16 | github-history | 10.0 / 10.0 | 36.1k / 17.6k | 20.3k / 15.3k | 6.6k / 3.2k | 2 / 2 | 13s / 14s | $0.041 / $0.031 | 2.77 / 5.67 |
| G17 | artifact | 10.0 / 10.0 | 35.1k / 14.4k | 23.3k / 20.5k | 5.7k / 4.8k | 2 / 1 | 14s / 15s | $0.047 / $0.041 | 2.85 / 6.95 |
| G18 | artifact | 10.0 / 10.0 | 46.0k / 26.2k | 23.3k / 19.7k | 6.7k / 7.1k | 5 / 4 | 16s / 16s | $0.047 / $0.039 | 2.17 / 3.81 |
| L21 | local-symbol | 8.0 / 9.0 | 36.3k / 27.3k | 24.4k / 25.1k | 6.5k / 7.8k | 3 / 3 | 15s / 20s | $0.049 / $0.050 | 2.20 / 3.30 |
| L22 | local-symbol | 9.5 / 9.5 | 24.1k / 23.2k | 18.3k / 15.8k | 4.3k / 3.7k | 2 / 3 | 9s / 16s | $0.037 / $0.032 | 3.94 / 4.09 |
| L23 | local-structure | 8.0 / 7.0 | 35.4k / 17.3k | 23.2k / 15.3k | 5.8k / 2.8k | 2 / 2 | 15s / 12s | $0.046 / $0.031 | 2.26 / 4.04 |
| L24 | local-structure | 7.0 / 9.0 | 58.7k / 39.9k | 39.4k / 31.3k | 19.1k / 15.7k | 6 / 5 | 21s / 21s | $0.079 / $0.063 | 1.19 / 2.26 |
| L25 | local-described-target | 10.0 / 10.0 | 41.6k / 25.8k | 28.0k / 21.1k | 11.8k / 6.3k | 2 / 3 | 14s / 14s | $0.056 / $0.042 | 2.41 / 3.88 |
| L26 | local-described-target | 9.5 / 9.0 | 80.5k / 21.0k | 34.1k / 20.7k | 21.0k / 6.3k | 6 / 2 | 17s / 14s | $0.068 / $0.041 | 1.18 / 4.29 |
| L27 | mixed | 9.0 / 10.0 | 51.6k / 45.8k | 30.5k / 29.8k | 11.9k / 16.4k | 4 / 6 | 19s / 24s | $0.061 / $0.060 | 1.74 / 2.18 |
| L28 | mixed | 9.5 / 10.0 | 39.9k / 48.7k | 28.8k / 32.9k | 10.2k / 19.3k | 4 / 5 | 14s / 25s | $0.058 / $0.066 | 2.38 / 2.05 |
| L29 | mixed | 9.0 / 8.0 | 55.6k / 51.0k | 30.8k / 35.1k | 15.8k / 21.6k | 4 / 5 | 15s / 26s | $0.062 / $0.070 | 1.62 / 1.57 |
| L30 | mixed | 10.0 / 10.0 | 57.4k / 26.5k | 29.0k / 26.4k | 17.7k / 11.8k | 3 / 3 | 16s / 15s | $0.058 / $0.053 | 1.74 / 3.77 |
| L31 | mixed | 9.0 / 10.0 | 22.2k / 25.7k | 14.5k / 18.9k | 2.3k / 6.1k | 1 / 4 | 9s / 13s | $0.029 / $0.038 | 4.06 / 3.89 |

## Tool usage

- **octocode**: localSearch 55, localFetch 51, ghGetFileContent 20, ghGetHistoryItem 18, ghSearchCode 8, structureSearch 2, ghStructure 2, ghSearchHistory 2, artifactSearch 2, astSearch 1 · counters: clasify calls 0, matchString queries 21, lspSearch calls 0, astSearch calls 1 · tool errors 4 · permission denials 0
- **rg-gh**: Bash:invocation 161 · tool errors 15 · permission denials 0

Per-question counters (non-zero):

- G05: octocode.matchString queries=1
- G06: octocode.matchString queries=2
- G07: octocode.matchString queries=2
- G10: octocode.matchString queries=1
- L09: octocode.matchString queries=3
- L11: octocode.matchString queries=1
- L13: octocode.matchString queries=2
- L17: octocode.matchString queries=1
- G13: octocode.matchString queries=3
- G14: octocode.matchString queries=1
- G17: octocode.matchString queries=1
- G18: octocode.matchString queries=2
- L23: octocode.astSearch calls=1
- L26: octocode.matchString queries=1

## Judge agreement

49 question-pairs judged; 0 needed a tie-break; 0 grader errors. Order-swap spread per worker score: mean 0.19, max 1; 98/98 within 1 point, 98/98 within 2. Preferred answer consistent across both orders: 34/49.

Reference issues raised by the judge:

- G03: Minor: the reference quotes the removed base comment only partially; its full text also says 'but the client cannot be closed in this state'. Otherwise the reference is consistent with the diff.
- G10: The reference omits that net_lookup_host's resolve_dns (no getaddrinfo) and io_driver, io_driver_drop, net_bind_resource, no_rt and rt_handle_block_on are also enabled; this is minor.
- G10: Minor: does not mention that the resolve_dns test in net_lookup_host.rs was re-enabled (its 'No getaddrinfo' ignore was removed).
- G16: Key fact 5 gives the label as `__unix_socket__`. The PR #19399 body says `__scrape_unix_socket__`. This could not be checked against the source at ea954809ce. It is minor and does not affect grading.
- G16: Key fact 5 names the label `__unix_socket__`. PR #19399's body says `__scrape_unix_socket__`, which matches both answers, so the reference's label name may be inaccurate (not checked in the code at the commit).
- L24: The reference omits (*Manager).Stop (manager.go:346-353), which starts goroutines via errgroup.Group.Go to stop pools in parallel, limited to GOMAXPROCS. 'Exactly six functions' holds only for literal go statements.
- L24: The reference omits (*Manager).Stop, which starts goroutines through errgroup.Group.Go at manager.go:346-353, each calling sp.stop() with concurrency limited to GOMAXPROCS. 'Exactly six functions' is true only for literal go statements.
- L28: Minor: the reference's '+18 −6' is the src/t_array.c count only; the whole PR is +45 −6 including 27 lines of tests.
- L30: Minor: the reference leaves out the wraparound_unbounded test the PR also adds.

## Wrong claims flagged

- G01 octocode: Overstates that every app automatically gets spans/metrics/logs (inert without providers)
- G01 rg-gh: Slight overstatement that telemetry is emitted with 'no opt-in' for every app (inert until providers are configured), though enabled() bypass is mentioned | Lists 'excluded' as a bypass condition in FastAPI.__call__ (minor; exclusion is handled inside NativeTelemetry)
- G04 octocode: click.option("--0-file") warns — an exposed option with a non-identifier derived name still raises TypeError; it only warns when expose_value=False | click.option("--0-file") warns 'not a valid Python identifier' — with default expose_value=True it still raises TypeError; only unexposed options warn
- G05 rg-gh: Describes should_strip_auth as stripping on any scheme/port change except http->https default ports, omitting the same-scheme default-port/None exception (minor imprecision)
- G10 rg-gh: 36 files changed (actually 37) | net_lookup_host listed among TCP tests (it is the DNS/getaddrinfo test) | Says 36 files changed; PR has 37
- L02 octocode: Says the fields filter is used 'as with update_fields'; save() never passes fields, only bulk_update does | Implies the fields filter applies 'as with update_fields'; save() never passes fields, only bulk_update does
- L02 rg-gh: Error message stated as hardcoded 'save() prohibited...' although the prefix is operation_name (minor) | Quotes the message prefix as 'save()' although it is the operation_name argument (minor)
- L03 rg-gh: Callbacks registered by a running callback 'go onto the new list' — in autocommit mode (no atomic block) they actually run immediately; only true if registered inside a new atomic block | Callbacks registered by a running callback go onto the new list (they actually run immediately: autocommit is already True at base.py:488 and the connection is no longer in an atomic block)
- L04 octocode: self/cls stripping listed as applied on the from_function path (filter_args is always non-empty there) | Lists self/cls stripping among filtered args on this path; from_function always passes non-empty filter_args so the self/cls branch (base.py:324-329) is not taken
- L04 rg-gh: self/cls stripping listed as applied on the from_function path (filter_args is always non-empty there) | Says create_schema_from_function drops self/cls for methods; not applicable on the from_function path
- L05 octocode: Implies test_merge_content parameters would need rewriting; all cases have single-item others, so the test still passes
- L05 rg-gh: test_merge_content 'would need updating either way' — every case has a single-item others list, so the test still passes
- L08 octocode: Suggests the deferred func at scrape/scrape.go:1829 handles the returned error (hedged); it only skips cache iterDone on error. The rollback is in scrapeAndReport at line 1620. | Says the returned error is handled by the deferred function at scrape/scrape.go:1829; that defer only skips cache iterDone on error, and rollback happens in scrapeAndReport (~1620). Hedged.
- L12 octocode: Minor imprecision: says displacement is the only notify case 'on this path' (non-LIFO branch also notifies)
- L12 rg-gh: Minor: says slot task moved to run queue when core 'shut down' citing 479-481, which is only the block_in_place handoff
- L13 octocode: Summary states a DEL is propagated; it is UNLINK when lazyfree-lazy-expire is on (later hedged)
- L15 octocode: Line 446 described as likely Builder copy-on-write path; it is actually forceJdk() test hook (hedged) | Line 446 described as Builder copy-on-write path; it is the forceJdk() test hook (hedged)
- L16 rg-gh: Says each segment keeps at least about 20 weight units; the x20 check precedes doubling, so the guarantee is ~10 per segment | 'each segment keeps at least about 20 weight units' — the x20 condition yields roughly >=10 per segment
- G12 octocode: __init__.py re-exports 14 names (actually 15; its own list has 15) | States __init__.py re-exports 14 names; it actually re-exports 15 (and X's own list has 15)
- G15 rg-gh: Lists #8067 under 'Re-land' as if the re-land were still pending, which is ambiguous next to saying #8337 re-landed it
- L21 octocode: Claims there are five functions named begin_shutdown and four run per shutdown; there are four and three run (its own list contradicts the count) | Says there are five functions named begin_shutdown and that four run per shutdown; there are four, and three run. Its own list excludes BlockingPool::shutdown, so the count contradicts itself.
- L23 octocode: Headline says twenty-five methods; table (correctly) lists 23 | Headline says twenty-five methods; actual (and its own table) is 23
- L23 rg-gh: Headline says thirty methods; table lists 23 | Headline says thirty QuerySet methods call self._chain() directly; actual (and its own table) is 23 | Says 'The def line shown for each hit' though no def lines are shown
- L24 octocode: Attributes the goroutines at manager.go:434 to (*Manager).reload instead of (*Manager).ApplyConfig (hedged), so it lists reload twice and never names ApplyConfig | Attributes the goroutines at manager.go:434-458 to (*Manager).reload; they are in ApplyConfig | Lists reload twice, so the claimed six functions is wrong and ApplyConfig is never named
- L24 rg-gh: Opening says 'Five functions' before correcting itself to six (cosmetic) | Opening says 'Five functions' before correcting itself to six (contradiction in the text)
- L26 octocode: save_base located 'around line 1075' (it is at 956; call at 1002) — minor citation slip | Cites save_base 'around line 1075' (that is _save_table; save_base is at 956 and calls _save_table at 1002). The claim itself is right; only the line is wrong.
- L29 rg-gh: Point 4: says a panic in poll_acquire's assign_permits (line 488) 'leaves the same state'. On the first poll the node is not linked, so that path leaks permits rather than causing the use-after-free. | Says a panic in the poll_acquire assign_permits event 'leaves the same state' without saying this only holds when the node is already queued (when not queued it is a permit leak)
- L30 octocode: Labels chan::channel_from_list as test-only (it is a non-test helper) — minor | Minor: calls list::channel_from_index and chan::channel_from_list 'test-only code' though they are not cfg(test)-gated
- L30 rg-gh: Minor: groups list::channel_from_index and chan::channel_from_list under 'test-only additions' though they are not cfg(test)-gated

## Cost

Worker research $5.56 (octocode $2.97, rg-gh $2.59) · judge $11.57 · reflections $4.80 · probes $0.17 · reflection synthesis $0.00 · reported Claude total $22.10. Classification provider cost and total system cost remain unknown.

## Isolation

Every run passed its profile isolation check (offered/called tools, MCP servers, no skills, no benchmark-file access).
