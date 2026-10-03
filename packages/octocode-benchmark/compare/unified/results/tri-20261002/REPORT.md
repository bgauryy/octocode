# Unified benchmark — run `tri-20261002`

Workers: `octocode`, `octocode-npm`, `rg-gh` · model claude-sonnet-5-5 · 30 questions × 1 pass · judge claude-opus-5-5 (blinded X/Y, both orders, tie-break when spread > 2).
Build: MCP dist sha256 `de2b10c3c802` · Claude Code 2.1.286 (Claude Code).

Quality 0–10 = correctness 0–5 + completeness 0–3 + evidence 0–2. Tokens: total = all input kinds + output; research is an estimate: total − (first-request context × requests). Weighted tokens are unknown without a frozen model tariff and verified cache TTL. Cost and tokens cover the worker Claude session; classifier provider totals are separate/unknown. Efficiency = quality per 10k total tokens; weighted efficiency = quality per 10k weighted tokens.

## Totals

| worker | mean quality | median quality | total tokens | weighted tokens | research tokens | output tokens | requests | tool calls | cost | time | efficiency | weighted efficiency |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| octocode | 8.99 | 9.0 | 1672.4k | — | 560.6k | 59.3k | 112 | 112 | $2.25 | 9.2 min | 1.61 | — |
| octocode-npm | 8.80 | 9.0 | 5194.8k | — | 740.3k | 58.4k | 122 | 105 | $3.24 | 10.1 min | 0.51 | — |
| rg-gh | 8.93 | 8.9 | 918.5k | — | 403.1k | 50.7k | 109 | 80 | $1.73 | 8.9 min | 2.91 | — |

Per-question ratio octocode/octocode-npm: total tokens mean 0.34× / median 0.33×; weighted tokens mean —× / median —×; research tokens mean 0.91× / median 0.75×; cost mean 0.71× / median 0.71×. Quality delta (octocode − octocode-npm): mean 0.19, wins/ties/losses 9/17/4.
Per-question ratio octocode/rg-gh: total tokens mean 1.93× / median 1.88×; weighted tokens mean —× / median —×; research tokens mean 1.55× / median 1.43×; cost mean 1.32× / median 1.26×. Quality delta (octocode − rg-gh): mean 0.07, wins/ties/losses 6/19/5.
Per-question ratio octocode-npm/rg-gh: total tokens mean 6.14× / median 5.72×; weighted tokens mean —× / median —×; research tokens mean 2.19× / median 1.88×; cost mean 1.94× / median 1.84×. Quality delta (octocode-npm − rg-gh): mean -0.13, wins/ties/losses 7/14/9.

## Fixed overhead ("reply OK" probe)

| worker | first-request context | tools offered | isolation probe |
|---|--:|--:|---|
| octocode | 9.8k | 13 | pass |
| octocode-npm | 36.4k | 9 | pass |
| rg-gh | 4.6k | 1 | pass |

## By category

| category | n | octocode quality | octocode tokens | octocode research | octocode-npm quality | octocode-npm tokens | octocode-npm research | rg-gh quality | rg-gh tokens | rg-gh research |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| github | 10 | 8.47 | 49.3k | 18.7k | 8.53 | 145.1k | 21.1k | 8.65 | 32.9k | 16.1k |
| local | 20 | 9.25 | 59.0k | 18.7k | 8.94 | 187.2k | 26.4k | 9.06 | 29.5k | 12.1k |
| github-bug-rca | 2 | 8.38 | 38.6k | 9.0k | 8.75 | 140.0k | 12.5k | 8.50 | 35.8k | 17.2k |
| github-code-research | 3 | 8.50 | 46.4k | 13.5k | 8.67 | 149.0k | 15.4k | 8.75 | 31.4k | 12.7k |
| github-pr-review | 5 | 8.50 | 55.3k | 25.7k | 8.35 | 144.7k | 28.1k | 8.65 | 32.6k | 17.7k |
| local-impact | 2 | 9.50 | 60.7k | 25.8k | 8.75 | 97.6k | 6.2k | 9.38 | 21.2k | 6.9k |
| local-locate | 1 | 9.00 | 46.0k | 6.2k | 10.00 | 194.3k | 11.6k | 9.00 | 17.5k | 3.2k |
| local-semantic | 6 | 9.50 | 55.5k | 17.4k | 9.25 | 184.7k | 26.4k | 9.00 | 30.4k | 13.0k |
| local-trace | 11 | 9.09 | 61.7k | 19.2k | 8.70 | 204.2k | 31.5k | 9.05 | 31.6k | 13.4k |

Token columns are per-question means.

## Per question

Each cell shows octocode / octocode-npm / rg-gh.

| q | category | quality | total tokens | weighted tokens | research tokens | tool calls | time | cost | efficiency |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|
| G01 | github-pr-review | 9.0 / 7.3 / 10.0 | 100.3k / 136.8k / 55.4k | — / — / — | 50.9k / 27.5k / 36.7k | 4 / 3 / 4 | 32s / 29s / 39s | $0.153 / $0.151 / $0.133 | 0.90 / 0.53 / 1.81 |
| G02 | github-pr-review | 7.8 / 8.0 / 8.3 | 54.3k / 254.6k / 44.5k | — / — / — | 24.7k / 35.9k / 25.8k | 2 / 5 / 3 | 18s / 25s / 23s | $0.092 / $0.140 / $0.092 | 1.43 / 0.31 / 1.86 |
| G03 | github-pr-review | 8.8 / 9.0 / 8.8 | 28.6k / 82.2k / 17.7k | — / — / — | 8.8k / 9.4k / 8.4k | 1 / 1 / 1 | 13s / 15s / 14s | $0.058 / $0.070 / $0.053 | 3.06 / 1.09 / 4.93 |
| G04 | github-pr-review | 8.3 / 8.5 / 8.0 | 29.6k / 94.2k / 19.6k | — / — / — | 9.9k / 21.4k / 5.7k | 1 / 1 / 2 | 13s / 14s / 16s | $0.061 / $0.119 / $0.039 | 2.79 / 0.90 / 4.08 |
| G05 | github-code-research | 8.8 / 8.0 / 8.8 | 46.1k / 203.1k / 44.5k | — / — / — | 16.6k / 20.9k / 21.3k | 3 / 4 / 4 | 21s / 22s / 23s | $0.081 / $0.107 / $0.068 | 1.90 / 0.39 / 1.97 |
| G06 | github-code-research | 7.8 / 9.0 / 8.0 | 49.0k / 119.9k / 25.8k | — / — / — | 9.6k / 10.6k / 7.2k | 4 / 2 / 3 | 20s / 19s / 15s | $0.054 / $0.070 / $0.042 | 1.58 / 0.75 / 3.10 |
| G07 | github-code-research | 9.0 / 9.0 / 9.5 | 44.0k / 124.0k / 23.7k | — / — / — | 14.5k / 14.6k / 9.7k | 2 / 2 / 2 | 23s / 22s / 17s | $0.067 / $0.087 / $0.059 | 2.04 / 0.73 / 4.01 |
| G08 | github-bug-rca | 7.8 / 9.0 / 8.0 | 34.8k / 116.3k / 19.0k | — / — / — | 5.2k / 7.1k / 5.0k | 2 / 3 / 2 | 11s / 14s / 15s | $0.039 / $0.061 / $0.036 | 2.23 / 0.77 / 4.21 |
| G09 | github-bug-rca | 9.0 / 8.5 / 9.0 | 42.4k / 163.7k / 52.7k | — / — / — | 12.8k / 18.0k / 29.4k | 2 / 3 / 4 | 16s / 21s / 25s | $0.069 / $0.103 / $0.076 | 2.12 / 0.52 / 1.71 |
| G10 | github-pr-review | 8.8 / 9.0 / 8.3 | 63.9k / 155.6k / 25.8k | — / — / — | 34.2k / 46.2k / 11.7k | 2 / 2 / 2 | 18s / 22s / 19s | $0.127 / $0.160 / $0.068 | 1.37 / 0.58 / 3.20 |
| L01 | local-trace | 9.0 / 9.5 / 10.0 | 38.0k / 120.5k / 19.5k | — / — / — | 8.2k / 10.9k / 5.3k | 2 / 2 / 2 | 13s / 13s / 12s | $0.048 / $0.071 / $0.038 | 2.37 / 0.79 / 5.12 |
| L02 | local-locate | 9.0 / 10.0 / 9.0 | 46.0k / 194.3k / 17.5k | — / — / — | 6.2k / 11.6k / 3.2k | 4 / 5 / 2 | 15s / 21s / 13s | $0.048 / $0.088 / $0.032 | 1.96 / 0.51 / 5.15 |
| L03 | local-trace | 8.8 / 9.8 / 9.3 | 67.3k / 261.6k / 22.9k | — / — / — | 17.6k / 42.5k / 8.7k | 7 / 6 / 2 | 23s / 24s / 15s | $0.073 / $0.132 / $0.051 | 1.30 / 0.37 / 4.04 |
| L04 | local-trace | 8.3 / 8.0 / 7.8 | 125.6k / 288.8k / 58.3k | — / — / — | 55.8k / 69.5k / 34.4k | 8 / 5 / 4 | 28s / 29s / 21s | $0.129 / $0.167 / $0.089 | 0.66 / 0.28 / 1.33 |
| L05 | local-impact | 9.0 / 8.5 / 8.8 | 35.2k / 77.9k / 19.8k | — / — / — | 5.3k / 4.7k / 5.4k | 2 / 1 / 2 | 10s / 12s / 12s | $0.040 / $0.051 / $0.040 | 2.55 / 1.09 / 4.43 |
| L06 | local-trace | 9.0 / 8.8 / 8.8 | 54.2k / 163.5k / 28.4k | — / — / — | 14.4k / 17.4k / 9.5k | 3 / 3 / 3 | 15s / 19s / 17s | $0.055 / $0.082 / $0.044 | 1.66 / 0.54 / 3.08 |
| L07 | local-trace | 9.3 / 9.0 / 8.8 | 44.3k / 124.8k / 24.9k | — / — / — | 14.4k / 15.2k / 10.6k | 6 / 3 / 2 | 16s / 17s / 15s | $0.075 / $0.091 / $0.055 | 2.09 / 0.72 / 3.51 |
| L08 | local-trace | 9.0 / 8.0 / 9.0 | 51.2k / 211.0k / 20.1k | — / — / — | 11.5k / 28.4k / 5.9k | 5 / 5 / 2 | 17s / 19s / 13s | $0.059 / $0.112 / $0.042 | 1.76 / 0.38 / 4.47 |
| L09 | local-semantic | 9.8 / 9.0 / 9.8 | 77.2k / 220.9k / 59.6k | — / — / — | 27.4k / 38.1k / 31.0k | 7 / 6 / 5 | 24s / 27s / 26s | $0.098 / $0.138 / $0.079 | 1.26 / 0.41 / 1.64 |
| L10 | local-semantic | 10.0 / 9.0 / 9.0 | 40.8k / 123.9k / 21.7k | — / — / — | 10.9k / 14.3k / 7.4k | 3 / 2 / 2 | 16s / 18s / 15s | $0.069 / $0.096 / $0.051 | 2.45 / 0.73 / 4.15 |
| L11 | local-trace | 10.0 / 10.0 / 9.3 | 82.6k / 223.6k / 35.1k | — / — / — | 32.8k / 40.9k / 16.1k | 8 / 4 / 3 | 28s / 24s / 18s | $0.101 / $0.137 / $0.063 | 1.21 / 0.45 / 2.63 |
| L12 | local-semantic | 9.3 / 9.5 / 8.0 | 44.5k / 220.9k / 22.1k | — / — / — | 14.6k / 38.0k / 7.8k | 2 / 5 / 2 | 17s / 25s / 16s | $0.065 / $0.134 / $0.050 | 2.08 / 0.43 / 3.62 |
| L13 | local-trace | 8.8 / 6.8 / 7.8 | 55.7k / 210.1k / 43.1k | — / — / — | 15.9k / 27.5k / 19.3k | 4 / 5 / 4 | 18s / 21s / 20s | $0.071 / $0.115 / $0.064 | 1.57 / 0.32 / 1.80 |
| L14 | local-semantic | 9.0 / 9.0 / 8.5 | 34.7k / 194.7k / 18.5k | — / — / — | 4.8k / 12.0k / 4.2k | 2 / 4 / 2 | 12s / 17s / 14s | $0.043 / $0.086 / $0.038 | 2.59 / 0.46 / 4.60 |
| L15 | local-semantic | 9.0 / 9.0 / 10.0 | 50.0k / 121.4k / 22.1k | — / — / — | 10.3k / 11.9k / 7.9k | 3 / 2 / 2 | 15s / 15s / 15s | $0.058 / $0.079 / $0.049 | 1.80 / 0.74 / 4.52 |
| L16 | local-trace | 9.0 / 8.0 / 10.0 | 58.6k / 172.9k / 30.0k | — / — / — | 18.8k / 26.8k / 10.9k | 5 / 3 / 3 | 18s / 18s / 18s | $0.073 / $0.103 / $0.056 | 1.54 / 0.46 / 3.33 |
| L17 | local-trace | 10.0 / 9.0 / 9.0 | 38.4k / 212.4k / 30.1k | — / — / — | 8.5k / 29.6k / 11.0k | 3 / 5 / 3 | 18s / 22s / 20s | $0.056 / $0.120 / $0.051 | 2.60 / 0.42 / 2.99 |
| L18 | local-semantic | 10.0 / 10.0 / 8.8 | 86.0k / 226.5k / 38.5k | — / — / — | 36.2k / 43.8k / 19.5k | 5 / 5 / 3 | 21s / 22s / 19s | $0.104 / $0.142 / $0.062 | 1.16 / 0.44 / 2.27 |
| L19 | local-trace | 9.0 / 9.0 / 10.0 | 63.0k / 257.4k / 34.9k | — / — / — | 13.2k / 38.0k / 15.8k | 5 / 6 / 3 | 18s / 25s / 19s | $0.069 / $0.162 / $0.060 | 1.43 / 0.35 / 2.86 |
| L20 | local-impact | 10.0 / 9.0 / 10.0 | 86.2k / 117.3k / 22.6k | — / — / — | 46.4k / 7.6k / 8.3k | 5 / 2 / 2 | 24s / 14s / 14s | $0.116 / $0.063 / $0.046 | 1.16 / 0.77 / 4.42 |

## Tool usage

- **octocode**: localFetch 52, localSearch 35, ghGetHistoryItem 12, ghGetFileContent 9, ghSearchCode 2, structureSearch 2 · counters: clasify calls 0, matchString queries 9, lspSearch calls 0, astSearch calls 0 · tool errors 12 · permission denials 0
- **octocode-npm**: localSearch 45, localGetFileContent 35, ghGetHistoryItem 14, ghGetFileContent 9, ghSearchHistory 2 · counters: clasify calls 0, matchString queries 15, lspSearch calls 0, astSearch calls 0 · tool errors 4 · permission denials 0
- **rg-gh**: Bash:invocation 80 · tool errors 1 · permission denials 0

Per-question counters (non-zero):

- G01: octocode.matchString queries=2
- G05: octocode.matchString queries=2; octocode-npm.matchString queries=3
- G06: octocode.matchString queries=1; octocode-npm.matchString queries=4
- G07: octocode.matchString queries=1; octocode-npm.matchString queries=2
- G10: octocode.matchString queries=1
- L02: octocode-npm.matchString queries=1
- L09: octocode.matchString queries=2; octocode-npm.matchString queries=1
- L11: octocode-npm.matchString queries=1
- L14: octocode-npm.matchString queries=1
- L16: octocode-npm.matchString queries=1
- L20: octocode-npm.matchString queries=1

## Judge agreement

90 question-pairs judged; 1 needed a tie-break; 0 grader errors. Order-swap spread per worker score: mean 0.28, max 3; 176/180 within 1 point, 179/180 within 2. Preferred answer consistent across both orders: 69/90.

Reference issues raised by the judge:

- G03: The reference paraphrases the old comment as 'as it will close the client'. The full comment continues 'but the client cannot be closed in this state', which both answers quote correctly. Minor.
- G05: Reference line numbers are about 3 lower than the fetched file at some locations (for example, resolve_redirects body), which is within tolerance.
- G09: Minor: the reference doesn't mention the alternative unmerged PRs (#13787, #13794); doesn't affect grading.
- G10: Minor: the reference phrases the #5054 exclusion as a tcp_stream test exclusion; in the diff only a vectored-I/O section of try_read_write is #[cfg(not(miri))]. It also omits that non-TCP-named files (io_driver, io_driver_drop, no_rt, net_bind_resource, rt_handle_block_on) were enabled.
- L05: The Acceptable variations section says test_messages.py:1109 'would break', but every parametrized case passes exactly one item in `others`, so the test would still pass with a two-argument signature.
- L05: The acceptable variations say test_messages.py:1109 'would break', but every parametrized case at lines 1085-1101 has exactly one item in others, so the call passes exactly two arguments and would still work with a two-argument signature.
- L11: Minor gap: the reference doesn't mention that Core::poll sets the stage to Consumed when the future is Ready (core.rs:384-386), before poll_future stores Finished. The reference's description of cancel_task as storing JoinError::cancelled is simplified. It actually stores panic_result_to_join_error,
- L20: Reference says only the config parser writes the raw ResolveJsonModule field, but tsc/internal/project/project.go:218 also sets ResolveJsonModule: core.TSTrue (inferred project options).
- L20: The reference says there are exactly four non-test call sites, but it misses tsoptions/showconfig.go:47, which uses GetResolveJsonModule as a method expression (computeFn((*core.CompilerOptions).GetResolveJsonModule)); the rg pattern with '\(' can't match it. The reference also says nothing else rea
- L20: Reference claims definition + 4 call sites is the full non-test set, but tsoptions/showconfig.go:47 also uses GetResolveJsonModule as a method value (computeFn((*core.CompilerOptions).GetResolveJsonModule)) for --showConfig computed output; the suggested rg pattern with '\(' misses it.
- L20: Reference says the four call sites are the full non-test set, but tsoptions/showconfig.go:47 also uses GetResolveJsonModule as a method value for --showConfig computed output; both answers correctly include it.

## Wrong claims flagged

- G01 octocode-npm: Unsupported auto-configure env settings raise FastAPIError that is caught and logged as a warning so startup still succeeds — actually the lifespan wrapper sends lifespan.startup.failed and re-raises  | Unsupported OTEL config raises FastAPIError that is caught and logged as a warning so startup still succeeds (actually it sends lifespan.startup.failed and re-raises) | Span names given as endpoint/dependencies etc. rather than fastapi.endpoint etc. (minor)
- G02 octocode: JSON schema 'unchanged' glosses over the constrained case (minLength vs minProperties per #13704) — minor
- G02 octocode-npm: Minor: Counter instance 'accepted as is'
- G02 rg-gh: Hedged misidentification of the new collections.Counter entry: it is _defaultdict_allowed_default_types, not a table of natively handled mapping types | Guessed that collections.Counter entry was added to a table for natively handled mapping types; it was added to _defaultdict_allowed_default_types (hedged) | Hedged guess that the new Counter entry goes in a table for natively handled mapping types; it is actually _defaultdict_allowed_default_types (minor)
- G03 octocode: Implies clientTtl-evicted clients also kept pulling queued requests via their drain listener; in the base code they were already closed via client.close(() => {}), so the TTL issue was only close()/de
- G03 rg-gh: Says the new test file checks three things; it has four tests per pool type and leaves out 'destroy() aborts in-flight requests on a client evicted by clientTtl'
- G04 octocode: Lists `--0-file` as a declaration that warns without noting that it must be unexposed; an exposed option with that name still raises TypeError | Lists `--0-file` (an exposed option) as triggering a warning; an exposed option whose derived name is not an identifier still raises TypeError
- G05 octocode: Simplified should_strip_auth: omits the same-scheme default-port/None case that does not strip (minor imprecision)
- G05 rg-gh: Simplified should_strip_auth: omits the same-scheme default-port/None case that does not strip (minor imprecision)
- G06 octocode: Minor: says `send` calls _transport_for_url at 1005; it is _send_single_request | Explicit proxy map shown as {"all://": proxy} rather than Proxy(proxy) (minor imprecision)
- G08 octocode: Slight overstatement that the getOwnPropertyDescriptor trap had 'no reactive dependency'; the old trap did call get(s) when a value descriptor and a source existed
- G08 rg-gh: Old getOwnPropertyDescriptor trap 'never created or read a per-property source' — it did call get(s) when a descriptor with value and a source both existed | Old getOwnPropertyDescriptor trap 'never ... read a per-property source' — it read via get(s) when a value descriptor and source existed, and via untracked source?.v otherwise | Says the old getOwnPropertyDescriptor trap 'never created or read a per-property source'; it actually read it untracked through source?.v and called tracked get(s) when a descriptor value and a source
- G09 octocode-npm: Implies #13787 was authored by Viicos ('The author's earlier PR'); it was by datrixlab | Calls #13787 'the author's earlier PR', implying Viicos wrote it; it was opened by datrixlab
- G10 octocode: Minor: says two rt_common tests ignored for host I/O; actually one test plus a helper fn | Minor: 'two rt_common tests' ignored for host I/O events; it is one test plus a helper fn | net_panic described as comment-only, though two of its ignores were removed
- G10 rg-gh: 'most of net_panic' newly enabled overstates it: only 2 of 7 ignores were removed | Describes the readv case as a separate 'readv-error test' when it is a cfg(not(miri)) block inside try_read_write (minor) | Counts two rt_common tests ignored for miri#5047; one is a helper fn (minor)
- L03 octocode: Callbacks registered by a running callback are 'queued for the next round'; in fact autocommit is already True (base.py:488), so they run immediately | Callbacks registered by a running callback are queued for the next round — actually autocommit is already True and not in atomic, so they run immediately | Callbacks registered by a running callback are queued for the next round; actually autocommit is already True and in_atomic_block False, so they execute immediately
- L03 octocode-npm: Says a callback registered by a running callback 'appends to the new list' before correctly saying it runs immediately; the reasoning given (flag reset timing) is muddled
- L03 rg-gh: transaction.py:~299-303 for connection.commit() is wrong; actual line is 276 (flagged as approximate)
- L04 octocode: Lists self/cls stripping as applying on this path; from_function always passes non-empty filter_args so base.py:324-329 is not reached | Lists self/cls stripping among filtered args, though that branch doesn't run on the from_function path | Lists self/cls stripping among the filters applied on this path; it is skipped because from_function always passes filter_args
- L04 octocode-npm: Lists self/cls dropping as applying on this path; not reached because filter_args is always non-empty from from_function | Lists self/cls stripping among filtered args, though that branch doesn't run on the from_function path (filter_args is always non-empty) | self/cls stripping applies on this path (it doesn't; from_function always passes non-empty filter_args)
- L04 rg-gh: Includes self/cls in the filter list on this path; it is skipped because from_function always passes filter_args | Implies self/cls filtering applies on the @tool path (it is skipped because from_function passes non-empty filter_args) | validate_arguments citation base.py:~308-318 is off (actual 292-303)
- L05 octocode: Claims test_merge_content has parametrized cases with more than two contents that would need to change; no such cases exist at lines 1082-1103
- L05 octocode-npm: Direct answer states the variadic test at test_messages.py:1109 would break, but all parametrized cases have exactly one item in `others` (later hedged) | Direct answer says the variadic test at test_messages.py:1109 would break; every parametrized case has exactly one item in others, so it would still pass (X does hedge this later)
- L05 rg-gh: Says test_merge_content would need rewriting; every case passes exactly one item in others, so it still works with a two-argument signature
- L06 octocode-npm: Section heading lists 'app routes' among rendered responses, but route handlers don't get ETags via this flag
- L06 rg-gh: Intro says it affects 'route responses', which is vague and slightly misleading because route handlers don't use generateEtags
- L08 octocode-npm: Swaps config.go:510 (actually GlobalConfig) and :827 (actually ScrapeConfig) labels | Swaps config.go:510 (actually GlobalConfig) and config.go:827 (actually ScrapeConfig) | Swaps config locations: says config.go:510 is the scrape-config SampleLimit and 827 the global default; it is the reverse
- L09 rg-gh: Infers that a failed scrape triggers the empty-body append at :1625; the failed scrape actually goes through the normal append call at :1613 with an empty body (hedged) | The list of report series that get StaleNaN in reportStale is incomplete (omits scrape_series_added and the extra metrics)
- L10 octocode-npm: Cites :438-442 (extendedHistogramRate tail) as where extrapolatedRate applies factor/GaugeType to native histograms — misattributed citation | Cites :438-442 for extrapolatedRate's histogram handling; those lines belong to extendedHistogramRate (Div by range, not the extrapolation factor) | Cites :438-442 as the extrapolatedRate histogram path applying the factor and setting GaugeType; those lines belong to extendedHistogramRate
- L11 rg-gh: Says Harness::poll calls poll_future directly; it goes via poll_inner (minor) | Slightly imprecise: Harness::poll is said to call poll_future directly (it goes through poll_inner)
- L12 octocode-npm: I/O driver wakes are cited as never using the slot; driver wakes during park on a worker thread do go through schedule_local (core is in context, park==None) | Minor: describes the worker.rs:478-484 hand-off as 'after spawn_blocking'; it is in block_in_place (though the code comment mentions spawn_blocking)
- L12 rg-gh: Labels the lifo_slot->run_queue move at worker.rs:479-482 as happening on worker shutdown; it is in block_in_place core handoff | Item 7 attributes moving the LIFO slot task to the run queue (worker.rs:478-484) to worker shutdown; that code is on the path that hands the core to another thread (block_in_place / spawn_blocking), n | Calls the worker.rs:479-482 path 'Shutdown'; it is the block_in_place core hand-off
- L13 octocode-npm: Attributes notification/keyModified/propagateDeletion/stat_expiredkeys to db.c:2898-2906 (actually in deleteKeyAndPropagate 2847-2895) | Summary says 'propagates a DEL' (it is DEL or UNLINK depending on lazyfree-lazy-expire) | Cites db.c:2898-2906 as the tail of the delete helper doing notify/keyModified/propagateDeletion/stat_expiredkeys; those lines are the one-line wrappers, and the logic is in deleteKeyAndPropagate (~28
- L13 rg-gh: Short answer says a DEL is propagated; it is UNLINK when lazyfree-lazy-expire is on (the body does say DEL or UNLINK) | Wording implies propagateDeletion sends immediately; it only queues the command, which is flushed at the end of call()
- L15 octocode-npm: Minor: cites the experimental-choice comment at 812-814; it is actually at 809-810
- L16 octocode-npm: Claims the x20 condition gives each segment at least ~20 entries and that the code comment ('at least 10') is off; after doubling it is >=10, so the comment is right | Claims the x20 guard gives each segment 'at least about 20 entries' contrary to the comment; the guard is checked before doubling, so each segment gets >=10 as the comment says | Claims the x20 check gives each segment at least ~20 entries and that the 'at least 10' comment is inaccurate; actually the check happens before doubling, so each segment gets at least ~10 entries
- L18 rg-gh: The 'net effect' that rejected elements are always absent ignores duplicate-key restoration; it also describes remove_discarded_value as only erasing, omitting the stash restore at :982 (uncertainty a

## Cost

Worker research $7.22 (octocode $2.25, octocode-npm $3.24, rg-gh $1.73) · judge $21.18 · reflections $5.53 · probes $0.15 · reflection synthesis $0.00 · reported Claude total $34.08. Classification provider cost and total system cost remain unknown.

## Isolation

Every run passed its profile isolation check (offered/called tools, MCP servers, no skills, no benchmark-file access).
