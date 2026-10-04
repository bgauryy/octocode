# Unified benchmark — run `s12-p2`

Workers: `octocode`, `rg-gh` · model claude-sonnet-5-5 · 49 questions × 1 pass · judge claude-opus-5-5 (blinded X/Y, both orders, tie-break when spread > 2).
Build: MCP dist sha256 `8a6c5f5213df` · Claude Code 2.1.288 (Claude Code).

Quality 0–10 = correctness 0–5 + completeness 0–3 + evidence 0–2. Tokens: total = all input kinds + output; research is an estimate: total − (first-request context × requests). Weighted tokens are input-token equivalents under the frozen tariff claude-sonnet-5-5@tri-20261002 ($2/M input, $4/M 1h cache write, $0.2/M cache read, $10/M output); "cold" writes each session's first cached prefix instead of reading it. Q/$ = summed quality per dollar. Cost and tokens cover the worker Claude session; classifier provider totals are separate/unknown. Efficiency = quality per 10k total tokens; weighted efficiency = quality per 10k weighted tokens.

## Totals

| worker | mean quality | median quality | total tokens | weighted tokens (warm / cold) | research tokens | output tokens | requests | tool calls | cost | time | efficiency | weighted efficiency (warm / cold) | Q/$ (warm / cold) |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| octocode | 9.00 | 9.0 | 2261.9k | 1472.0k / 2120.1k | 623.0k | 75.3k | 166 | 151 | $2.94 | 12.6 min | 1.95 | 3.00 / 2.08 | 149.8 / 104.0 |
| rg-gh | 8.88 | 9.0 | 1666.0k | 1335.3k / 1510.9k | 664.9k | 79.3k | 207 | 164 | $2.67 | 15.0 min | 2.61 | 3.26 / 2.88 | 162.9 / 143.9 |

Per-question ratio octocode/rg-gh: total tokens mean 1.52× / median 1.51×; weighted tokens mean 1.13× / median 1.10×; research tokens mean 1.13× / median 1.05×; cost mean 1.13× / median 1.10×. Quality delta (octocode − rg-gh): mean 0.12, wins/ties/losses 9/33/7.

## Fixed overhead ("reply OK" probe)

| worker | first-request context | tools offered | isolation probe |
|---|--:|--:|---|
| octocode | — | — | not run |
| rg-gh | — | — | not run |

## By category

| category | n | octocode quality | octocode tokens | octocode research | rg-gh quality | rg-gh tokens | rg-gh research |
|---|--:|--:|--:|--:|--:|--:|--:|
| github | 18 | 8.83 | 40.9k | 10.9k | 8.86 | 29.1k | 12.6k |
| local | 31 | 9.10 | 49.2k | 13.8k | 8.89 | 36.9k | 14.1k |
| artifact | 2 | 9.75 | 40.9k | 6.5k | 10.00 | 26.9k | 7.7k |
| github-bug-rca | 2 | 8.50 | 38.2k | 8.8k | 7.75 | 21.4k | 7.2k |
| github-code-research | 3 | 8.67 | 45.2k | 12.5k | 9.00 | 28.1k | 10.6k |
| github-history | 2 | 9.00 | 36.4k | 6.9k | 8.50 | 18.2k | 3.8k |
| github-pr-review | 5 | 8.30 | 44.3k | 18.8k | 8.30 | 47.0k | 28.9k |
| github-repo-discovery | 2 | 10.00 | 37.6k | 3.2k | 9.75 | 17.8k | 3.4k |
| github-structure | 2 | 8.50 | 36.3k | 6.8k | 9.50 | 17.7k | 3.4k |
| local-described-target | 2 | 10.00 | 38.8k | 9.0k | 8.75 | 24.1k | 7.0k |
| local-impact | 2 | 9.50 | 44.1k | 9.4k | 9.00 | 37.2k | 15.2k |
| local-locate | 1 | 10.00 | 45.8k | 6.2k | 8.00 | 23.7k | 4.4k |
| local-semantic | 6 | 8.92 | 50.5k | 15.9k | 9.08 | 32.3k | 12.9k |
| local-structure | 2 | 9.25 | 52.2k | 12.7k | 6.50 | 37.1k | 12.9k |
| local-symbol | 2 | 8.75 | 29.9k | 5.1k | 9.00 | 25.2k | 5.7k |
| local-trace | 11 | 8.82 | 56.5k | 16.9k | 9.00 | 42.2k | 17.5k |
| mixed | 5 | 9.30 | 45.3k | 13.5k | 9.50 | 42.7k | 16.2k |

Token columns are per-question means.

## Per question

Each cell shows octocode / rg-gh.

| q | category | quality | total tokens | weighted tokens | research tokens | tool calls | time | cost | efficiency |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|
| G01 | github-pr-review | 8.5 / 9.0 | 52.6k / 110.8k | 59.4k / 70.5k | 23.2k / 77.5k | 3 / 8 | 28s / 40s | $0.119 / $0.141 | 1.61 / 0.81 |
| G02 | github-pr-review | 8.0 / 8.5 | 53.8k / 63.1k | 44.7k / 48.1k | 24.4k / 39.3k | 2 / 4 | 16s / 29s | $0.089 / $0.096 | 1.49 / 1.35 |
| G03 | github-pr-review | 9.0 / 9.0 | 28.4k / 17.9k | 28.4k / 26.7k | 8.8k / 8.4k | 1 / 1 | 12s / 13s | $0.057 / $0.053 | 3.17 / 5.02 |
| G04 | github-pr-review | 7.5 / 8.0 | 29.3k / 18.2k | 30.2k / 27.5k | 9.7k / 8.7k | 1 / 1 | 12s / 13s | $0.060 / $0.055 | 2.56 / 4.40 |
| G05 | github-code-research | 9.0 / 9.0 | 44.9k / 33.4k | 37.2k / 30.7k | 15.5k / 14.4k | 2 / 3 | 19s / 18s | $0.074 / $0.061 | 2.00 / 2.69 |
| G06 | github-code-research | 8.0 / 8.0 | 46.9k / 25.5k | 22.7k / 20.4k | 7.7k / 6.4k | 3 / 3 | 17s / 18s | $0.045 / $0.041 | 1.71 / 3.14 |
| G07 | github-code-research | 9.0 / 10.0 | 43.6k / 25.3k | 34.1k / 28.3k | 14.2k / 11.0k | 2 / 3 | 19s / 21s | $0.068 / $0.057 | 2.06 / 3.95 |
| G08 | github-bug-rca | 8.0 / 8.0 | 34.4k / 20.1k | 19.4k / 19.6k | 5.0k / 5.9k | 2 / 2 | 10s / 13s | $0.039 / $0.039 | 2.33 / 3.98 |
| G09 | github-bug-rca | 9.0 / 7.5 | 42.0k / 22.8k | 33.8k / 23.8k | 12.6k / 8.6k | 2 / 2 | 13s / 17s | $0.068 / $0.048 | 2.14 / 3.29 |
| G10 | github-pr-review | 8.5 / 7.0 | 57.5k / 25.0k | 51.0k / 32.0k | 28.0k / 10.6k | 2 / 2 | 22s / 19s | $0.102 / $0.064 | 1.48 / 2.80 |
| L01 | local-trace | 9.0 / 9.0 | 35.8k / 25.7k | 20.6k / 21.3k | 6.2k / 6.3k | 2 / 3 | 12s / 16s | $0.041 / $0.043 | 2.51 / 3.51 |
| L02 | local-locate | 10.0 / 8.0 | 45.8k / 23.7k | 22.3k / 18.2k | 6.2k / 4.4k | 4 / 3 | 14s / 18s | $0.045 / $0.036 | 2.19 / 3.37 |
| L03 | local-trace | 10.0 / 10.0 | 53.7k / 35.6k | 32.1k / 26.5k | 14.2k / 11.4k | 3 / 4 | 19s / 19s | $0.064 / $0.053 | 1.86 / 2.81 |
| L04 | local-trace | 8.0 / 8.0 | 113.0k / 77.7k | 62.9k / 48.7k | 53.5k / 43.6k | 10 / 6 | 26s / 27s | $0.126 / $0.097 | 0.71 / 1.03 |
| L05 | local-impact | 9.0 / 9.0 | 35.7k / 25.1k | 21.8k / 19.3k | 6.0k / 5.6k | 2 / 3 | 12s / 12s | $0.044 / $0.039 | 2.52 / 3.59 |
| L06 | local-trace | 8.5 / 9.0 | 36.5k / 35.0k | 23.6k / 23.8k | 6.9k / 10.8k | 2 / 4 | 12s / 16s | $0.047 / $0.048 | 2.33 / 2.57 |
| L07 | local-trace | 9.0 / 10.0 | 61.0k / 44.3k | 38.5k / 32.8k | 21.4k / 19.9k | 7 / 4 | 17s / 19s | $0.077 / $0.066 | 1.48 / 2.26 |
| L08 | local-trace | 8.0 / 9.0 | 37.3k / 36.1k | 24.8k / 25.4k | 7.7k / 12.0k | 3 / 4 | 16s / 15s | $0.050 / $0.051 | 2.14 / 2.49 |
| L09 | local-semantic | 8.5 / 9.5 | 63.8k / 30.3k | 46.2k / 28.9k | 24.2k / 10.9k | 4 / 3 | 20s / 17s | $0.092 / $0.058 | 1.33 / 3.14 |
| L10 | local-semantic | 10.0 / 9.0 | 38.8k / 32.2k | 29.9k / 32.9k | 9.1k / 12.8k | 2 / 3 | 14s / 19s | $0.060 / $0.066 | 2.58 / 2.79 |
| L11 | local-trace | 9.5 / 10.0 | 76.0k / 55.0k | 41.0k / 38.4k | 26.5k / 25.9k | 4 / 5 | 18s / 24s | $0.082 / $0.077 | 1.25 / 1.82 |
| L12 | local-semantic | 9.0 / 9.0 | 61.2k / 32.8k | 38.6k / 30.4k | 21.6k / 13.3k | 3 / 3 | 17s / 19s | $0.077 / $0.061 | 1.47 / 2.75 |
| L13 | local-trace | 8.0 / 8.0 | 50.9k / 59.1k | 32.8k / 39.8k | 11.4k / 30.0k | 4 / 5 | 23s / 23s | $0.066 / $0.080 | 1.57 / 1.35 |
| L14 | local-semantic | 8.5 / 8.0 | 34.5k / 24.8k | 21.2k / 20.9k | 4.8k / 5.4k | 2 / 3 | 12s / 15s | $0.042 / $0.042 | 2.46 / 3.22 |
| L15 | local-semantic | 9.5 / 10.0 | 37.2k / 30.7k | 26.3k / 29.3k | 7.6k / 11.3k | 2 / 3 | 15s / 17s | $0.053 / $0.059 | 2.55 / 3.26 |
| L16 | local-trace | 9.0 / 8.0 | 40.9k / 29.7k | 28.4k / 26.6k | 11.2k / 10.2k | 2 / 3 | 16s / 17s | $0.057 / $0.053 | 2.20 / 2.70 |
| L17 | local-trace | 9.0 / 9.0 | 52.0k / 27.0k | 30.2k / 23.8k | 12.4k / 7.5k | 4 / 3 | 18s / 18s | $0.060 / $0.048 | 1.73 / 3.33 |
| L18 | local-semantic | 8.0 / 9.0 | 67.3k / 43.0k | 42.2k / 36.2k | 27.8k / 23.6k | 3 / 3 | 16s / 18s | $0.084 / $0.072 | 1.19 / 2.09 |
| L19 | local-trace | 9.0 / 9.0 | 64.1k / 39.6k | 37.4k / 28.8k | 14.6k / 15.3k | 6 / 4 | 18s / 22s | $0.075 / $0.058 | 1.40 / 2.27 |
| L20 | local-impact | 10.0 / 9.0 | 52.5k / 49.2k | 28.6k / 41.8k | 12.9k / 24.9k | 4 / 4 | 16s / 26s | $0.057 / $0.084 | 1.90 / 1.83 |
| G11 | github-structure | 9.0 / 9.0 | 36.4k / 17.1k | 27.3k / 14.8k | 6.9k / 2.8k | 7 / 2 | 17s / 12s | $0.055 / $0.030 | 2.47 / 5.25 |
| G12 | github-structure | 8.0 / 10.0 | 36.2k / 18.3k | 24.4k / 19.2k | 6.7k / 4.0k | 4 / 2 | 18s / 17s | $0.049 / $0.038 | 2.21 / 5.46 |
| G13 | github-repo-discovery | 10.0 / 9.5 | 43.1k / 17.6k | 17.5k / 13.9k | 3.8k / 3.2k | 3 / 2 | 12s / 10s | $0.035 / $0.028 | 2.32 / 5.41 |
| G14 | github-repo-discovery | 10.0 / 10.0 | 32.0k / 18.0k | 15.0k / 14.9k | 2.5k / 3.6k | 2 / 2 | 11s / 10s | $0.030 / $0.030 | 3.13 / 5.57 |
| G15 | github-history | 8.0 / 7.0 | 22.2k / 10.9k | 14.7k / 11.3k | 2.5k / 1.3k | 2 / 1 | 12s / 9s | $0.029 / $0.023 | 3.61 / 6.41 |
| G16 | github-history | 10.0 / 10.0 | 50.7k / 25.5k | 24.3k / 18.0k | 11.4k / 6.3k | 3 / 3 | 13s / 21s | $0.049 / $0.036 | 1.97 / 3.93 |
| G17 | artifact | 10.0 / 10.0 | 36.4k / 26.6k | 23.7k / 22.1k | 6.9k / 7.4k | 3 / 3 | 14s / 21s | $0.047 / $0.044 | 2.75 / 3.76 |
| G18 | artifact | 9.5 / 10.0 | 45.4k / 27.2k | 21.5k / 22.8k | 6.1k / 8.1k | 4 / 3 | 13s / 19s | $0.043 / $0.046 | 2.09 / 3.68 |
| L21 | local-symbol | 8.0 / 8.0 | 36.6k / 26.8k | 24.9k / 23.8k | 6.9k / 7.3k | 2 / 3 | 15s / 16s | $0.050 / $0.048 | 2.18 / 2.99 |
| L22 | local-symbol | 9.5 / 10.0 | 23.2k / 23.6k | 16.4k / 16.1k | 3.3k / 4.1k | 2 / 3 | 8s / 15s | $0.033 / $0.032 | 4.10 / 4.24 |
| L23 | local-structure | 8.5 / 8.0 | 48.7k / 19.5k | 24.9k / 20.0k | 9.2k / 5.0k | 3 / 2 | 15s / 13s | $0.050 / $0.040 | 1.75 / 4.10 |
| L24 | local-structure | 10.0 / 5.0 | 55.7k / 54.8k | 33.0k / 34.1k | 16.1k / 20.8k | 4 / 6 | 19s / 25s | $0.066 / $0.068 | 1.80 / 0.91 |
| L25 | local-described-target | 10.0 / 7.5 | 41.8k / 27.4k | 28.6k / 22.3k | 12.0k / 7.9k | 2 / 3 | 12s / 17s | $0.057 / $0.045 | 2.39 / 2.73 |
| L26 | local-described-target | 10.0 / 10.0 | 35.7k / 20.8k | 23.3k / 20.2k | 6.0k / 6.2k | 2 / 2 | 13s / 14s | $0.047 / $0.040 | 2.80 / 4.80 |
| L27 | mixed | 9.0 / 9.5 | 55.1k / 73.6k | 31.9k / 37.1k | 15.3k / 29.6k | 4 / 9 | 18s / 34s | $0.064 / $0.074 | 1.63 / 1.29 |
| L28 | mixed | 10.0 / 9.5 | 40.1k / 40.9k | 28.9k / 31.3k | 10.3k / 16.5k | 4 / 5 | 13s / 22s | $0.058 / $0.063 | 2.49 / 2.32 |
| L29 | mixed | 9.0 / 8.5 | 67.4k / 32.0k | 39.6k / 25.6k | 27.6k / 12.4k | 4 / 3 | 17s / 15s | $0.079 / $0.051 | 1.34 / 2.66 |
| L30 | mixed | 10.0 / 10.0 | 41.6k / 36.4k | 26.8k / 27.4k | 11.8k / 16.9k | 2 / 4 | 12s / 15s | $0.054 / $0.055 | 2.40 / 2.75 |
| L31 | mixed | 8.5 / 10.0 | 22.2k / 30.4k | 14.7k / 18.4k | 2.3k / 5.9k | 1 / 4 | 8s / 19s | $0.029 / $0.037 | 3.83 / 3.29 |

## Tool usage

- **octocode**: localSearch 47, localFetch 46, ghGetFileContent 21, ghGetHistoryItem 19, ghSearchCode 8, structureSearch 2, ghStructure 2, ghSearchHistory 2, artifactSearch 2, astSearch 2 · counters: clasify calls 0, matchString queries 19, lspSearch calls 0, astSearch calls 2 · tool errors 2 · permission denials 0
- **rg-gh**: Bash:invocation 164 · tool errors 12 · permission denials 0

Per-question counters (non-zero):

- G05: octocode.matchString queries=2
- G06: octocode.matchString queries=1
- G07: octocode.matchString queries=2
- G10: octocode.matchString queries=1
- L11: octocode.matchString queries=1
- L12: octocode.matchString queries=1
- G11: octocode.matchString queries=3
- G13: octocode.matchString queries=3
- G14: octocode.matchString queries=1
- G17: octocode.matchString queries=2
- G18: octocode.matchString queries=1
- L23: octocode.astSearch calls=1
- L24: octocode.astSearch calls=1
- L27: octocode.matchString queries=1

## Judge agreement

49 question-pairs judged; 0 needed a tie-break; 0 grader errors. Order-swap spread per worker score: mean 0.22, max 2; 97/98 within 1 point, 98/98 within 2. Preferred answer consistent across both orders: 38/49.

Reference issues raised by the judge:

- G01: Line numbers for __call__ are slightly off (the actual code is at applications.py:1199-1229); within tolerance.
- G10: Minor: the reference doesn't mention that the PR also removes the net_lookup_host getaddrinfo ignore and the net_panic TCP ignores.
- G10: Minor: the reference cites rust-lang/miri#5054 as an issue, but the tcp_stream.rs comment links it as a pull request (miri/pull/5054). Otherwise consistent with the diff.
- G12: Minor: the reference does not say that OAuth2PasswordRequestFormStrict subclasses OAuth2PasswordRequestForm (oauth2.py:162). Otherwise accurate.
- G16: The reference names the label __unix_socket__ (UnixSocketLabel), but the PR #19399 body calls it __scrape_unix_socket__. The grader could not fetch the source at ea954809ce to resolve this, so neither answer was penalised for quoting the PR body.
- G16: The reference names the label `__unix_socket__`. The PR #19399 description says `__scrape_unix_socket__`, so the label constant or its value in the reference may need checking. This does not affect grading.
- G17: Reference omits the 'security': [] extra (empty) present at setup.py:124; minor.
- L24: Manager.Stop (manager.go:346-353) starts goroutines for sp.stop() through errgroup.Group.Go, limited to GOMAXPROCS. A grep for go statements misses it, so the reference's claim of 'exactly six functions' is arguably incomplete.
- L24: The reference says 'exactly six functions' based on searching for the go keyword. Manager.Stop (manager.go:346-355) also starts goroutines through errgroup.Group.Go with a GOMAXPROCS limit, each calling sp.stop(), so a complete answer could reasonably include it.
- L30: The reference mentions only the `wraparound` test, but the PR also adds `wraparound_unbounded`. Otherwise accurate.

## Wrong claims flagged

- G01 octocode: Suggests ExceptionTelemetryMiddleware's except Exception catches HTTPException/validation errors before the handlers run; it sits outside ExceptionMiddleware, so handled exceptions never reach it | Minor: says only the 'otlp' exporter is supported, but 'none' is also accepted
- G01 rg-gh: Minor: implies startup fails on OTEL_TRACES_EXPORTER=console regardless of endpoint (only fails when an OTLP endpoint is set)
- G02 rg-gh: 'Strict mode does not propagate to keys and values' is dubious: config-level strict reaches the key/value schemas
- G04 octocode: click.option("--0-file") is listed as a declaration that warns; as an exposed option it still raises TypeError (core.py:3395-3404). The upgrade guide only uses it as a migration example with an explic
- G06 octocode: Minor: says `send` calls _transport_for_url at line 1005; it is _send_single_request
- G09 rg-gh: Claims @dataclass(config=...) also mutated (unverified, minor)
- G10 octocode: Names `tcp_shutdown::shutdown` as the SO_LINGER-ignored test; it is `shutdown_after_tcp_reset` | Leaves tcp_stream out of the gate-removal list and does not tie the readv leak (miri#5054) to tcp_stream
- G10 rg-gh: net_lookup_host stays ignored due to no getaddrinfo — the PR removes that ignore | Says net_lookup_host stays ignored because there is no getaddrinfo; the PR removes that ignore from resolve_dns | Says a whole tcp_stream test is cfg(not(miri)); only a block inside the test is gated
- L02 rg-gh: Error message rendered as hardcoded 'save()' rather than operation_name-formatted (minor) | Renders the error message as 'save() prohibited...', but the code uses a %s placeholder for the operation name (minor)
- L04 octocode: Lists self/cls stripping among the filters on this path (does not apply since from_function passes non-empty filter_args) | Lists self/cls filtering among steps applied (not on this path) | Slightly imprecise: says Returns:/Example: blocks are skipped, whereas all subsequent non-Args blocks are dropped
- L04 rg-gh: Says create_schema_from_function drops self/cls for methods on this path (does not apply here) | Implies self/cls stripping applies in create_schema_from_function on this path (it doesn't, since from_function always passes filter_args)
- L05 octocode: Function body range given as base.py:373-435 (actually ~370-406) — minor | Function body range given as base.py:373-435 (the actual body is ~380-406), a minor slip in an uncertainty note
- L06 octocode: config-shared.ts:2614 described as passing into server render options; it is a serialized config object, so the wording is imprecise | config-shared.ts:2614 described as passing into server render options (it is a config subset object) — minor
- L08 octocode: Speculates rollback handling lives between scrape.go lines 2066-2158 (it is in scrapeAndReport ~1613-1633); hedged as unverified | Speculation that commit/rollback handling lies between scrape.go:2066-2158 (it is in scrapeAndReport ~1613-1633); hedged but misleading
- L13 rg-gh: Replica branch cited at db.c:3036-3039 (actual code is at 3043-3046); minor citation issue, not a wrong fact
- L14 octocode: Says debounced() records lastArgs/lastThis/lastCallTime and then evaluates shouldInvoke; the code evaluates shouldInvoke first (10501) and records afterwards (10503-10505).
- L16 rg-gh: Worked example: maximumSize(100) with concurrencyLevel(16) gives 4 segments — the loop actually yields 8 (stops at 8 since 8*20=160>100) | maximumSize(100) with concurrencyLevel(16) gives 4 segments; the loop actually yields 8 (4*20=80<=100 doubles to 8, then 160>100 stops)
- L18 octocode: Slight overstatement: 'first inserts rejected values' / containers inserted at start regardless of start-callback verdict | Overview says rejected values are first inserted and then removed; rejected scalars are never inserted (handle_value returns early). Only placeholders and kept containers are inserted and later remove
- G12 octocode: States __init__.py re-exports 14 names, but lists (and the file has) 15 | Says __init__ re-exports 14 names; it re-exports 15 (and Y lists 15)
- G15 rg-gh: Revert went out with 1.52.0 prep #8045 (merged 2026-04-14); actually the revert merged 2026-04-16, after #8045, and was meant for v1.52.1 | The revert went out with the 1.52.0 prep #8045. Wrong: #8057 merged 04-16, after #8045 (04-14), so it shipped in 1.52.1.
- L21 octocode: Claims there are five begin_shutdown functions; there are four and Y describes only four | Claims there are five begin_shutdown functions; there are four
- L21 rg-gh: Names the impl-level functions LockedInner::begin_shutdown / ShardedInner::begin_shutdown; they are methods of LockedImpl / ShardedImpl | Cites shutdown_rx.wait at pool.rs:321 (actually 324) | Names the impl types LockedInner/ShardedInner; the locked impl is on LockedImpl (verified at pool.rs:588) and LockedInner is the mutex-guarded struct; sharded one presumably ShardedImpl (not verified)
- L23 octocode: Speculates __or__/__xor__ likely get their clone via __and__ or self.query (they call query._chain() on another queryset), though hedged
- L23 rg-gh: States 31 lines in 26 methods; its own table shows 23 methods and 24 call lines | Says 'Thirty-one lines ... in 26 methods'; its own table (and the source) shows 24 call sites in 23 methods
- L24 rg-gh: Attributes the scrape.go:399 goroutine to scrapePool.reload; it is in restartLoops (scrape.go:355) | Lists the manager.go:434 site under Manager.reload and does not name ApplyConfig with confidence | Self-contradictory count ('Five ... that is six'), and the list actually contains seven functions
- L25 rg-gh: equalArrays removes only the `other` entry (~5766); actually both array and other are deleted at 5763-5764 | equalObjects deletes cited at 5916-5917; actually 5923-5924 (5916 is the constructor check) | If only one side is on the stack the check fails and the result is false; actually the code falls through and continues the recursive comparison
- L27 rg-gh: Says config.go:730 compares the scrape config's value to UnsetValidation; it is actually in GlobalConfig.isZero, comparing the global value
- L29 octocode: Minor: groups line 481 (contended path) under 'uncontended fast path' leak
- L29 rg-gh: Says the poll_acquire Ready-path event (481-487) runs after assign_permits returns; it actually runs before node.assign_permits at line 488
- L30 octocode: Minor: labels channel_from_list and list::channel_from_index as 'test-only', but they are not gated | Groups channel_from_list and list::channel_from_index under 'all test-only' changes, but neither is cfg(test) (minor)
- L30 rg-gh: Minor: calls list::channel_from_index part of the 'test-only' changes, but it is not cfg(test)-gated | Puts list::channel_from_index under 'test-only changes', but it is not cfg(test) (minor)

## Cost

Worker research $5.61 (octocode $2.94, rg-gh $2.67) · judge $11.93 · reflections $4.83 · probes $0.00 · reflection synthesis $0.00 · reported Claude total $22.38. Classification provider cost and total system cost remain unknown.

## Isolation

Every run passed its profile isolation check (offered/called tools, MCP servers, no skills, no benchmark-file access).
