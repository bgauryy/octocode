# Unified benchmark — run `s12fix-p1`

Workers: `octocode`, `rg-gh` · model claude-sonnet-5-5 · 18 questions × 1 pass · judge claude-opus-5-5 (blinded X/Y, both orders, tie-break when spread > 2).
Build: MCP dist sha256 `39291b81b057` · Claude Code 2.1.288 (Claude Code).

Quality 0–10 = correctness 0–5 + completeness 0–3 + evidence 0–2. Tokens: total = all input kinds + output; research is an estimate: total − (first-request context × requests). Weighted tokens are input-token equivalents under the frozen tariff claude-sonnet-5-5@tri-20261002 ($2/M input, $4/M 1h cache write, $0.2/M cache read, $10/M output); "cold" writes each session's first cached prefix instead of reading it. Q/$ = summed quality per dollar. Cost and tokens cover the worker Claude session; classifier provider totals are separate/unknown. Efficiency = quality per 10k total tokens; weighted efficiency = quality per 10k weighted tokens.

## Totals

| worker | mean quality | median quality | total tokens | weighted tokens (warm / cold) | research tokens | output tokens | requests | tool calls | cost | time | efficiency | weighted efficiency (warm / cold) | Q/$ (warm / cold) |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| octocode | 9.11 | 9.0 | 788.9k | 593.5k / 805.1k | 249.2k | 27.0k | 55 | 47 | $1.19 | 5.0 min | 2.08 | 2.76 / 2.04 | 138.2 / 101.8 |
| rg-gh | 8.92 | 9.0 | 508.0k | 469.0k / 526.3k | 207.4k | 27.1k | 63 | 49 | $0.94 | 5.4 min | 3.16 | 3.42 / 3.05 | 171.1 / 152.5 |

Per-question ratio octocode/rg-gh: total tokens mean 1.66× / median 1.77×; weighted tokens mean 1.26× / median 1.23×; research tokens mean 1.39× / median 1.29×; cost mean 1.26× / median 1.23×. Quality delta (octocode − rg-gh): mean 0.19, wins/ties/losses 6/9/3.

## Fixed overhead ("reply OK" probe)

| worker | first-request context | tools offered | isolation probe |
|---|--:|--:|---|
| octocode | — | — | not run |
| rg-gh | — | — | not run |

## By category

| category | n | octocode quality | octocode tokens | octocode research | rg-gh quality | rg-gh tokens | rg-gh research |
|---|--:|--:|--:|--:|--:|--:|--:|
| github | 18 | 9.11 | 43.8k | 13.8k | 8.92 | 28.2k | 11.5k |
| local | 0 | — | — | — | — | — | — |
| artifact | 2 | 10.00 | 34.8k | 5.3k | 9.50 | 25.2k | 8.5k |
| github-bug-rca | 2 | 9.00 | 38.3k | 9.0k | 8.00 | 21.1k | 6.9k |
| github-code-research | 3 | 8.67 | 49.7k | 17.0k | 9.00 | 28.0k | 10.6k |
| github-history | 2 | 9.25 | 37.2k | 7.7k | 8.50 | 17.9k | 3.5k |
| github-pr-review | 5 | 8.80 | 57.6k | 28.1k | 8.80 | 42.9k | 24.8k |
| github-repo-discovery | 2 | 10.00 | 32.9k | 3.4k | 10.00 | 22.9k | 3.7k |
| github-structure | 2 | 8.75 | 32.9k | 3.5k | 8.75 | 17.5k | 3.2k |

Token columns are per-question means.

## Per question

Each cell shows octocode / rg-gh.

| q | category | quality | total tokens | weighted tokens | research tokens | tool calls | time | cost | efficiency |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|
| G01 | github-pr-review | 9.0 / 10.0 | 85.6k / 90.2k | 88.9k / 75.9k | 46.3k / 66.3k | 4 / 5 | 47s / 38s | $0.178 / $0.152 | 1.05 / 1.11 |
| G02 | github-pr-review | 8.0 / 9.0 | 53.8k / 45.2k | 57.8k / 51.5k | 24.4k / 26.2k | 2 / 4 | 17s / 26s | $0.116 / $0.103 | 1.49 / 1.99 |
| G03 | github-pr-review | 9.0 / 9.0 | 28.4k / 17.9k | 28.4k / 26.7k | 8.8k / 8.4k | 1 / 1 | 11s / 13s | $0.057 / $0.053 | 3.17 / 5.02 |
| G04 | github-pr-review | 8.0 / 7.0 | 29.4k / 19.7k | 30.3k / 19.5k | 9.8k / 5.4k | 1 / 2 | 10s / 15s | $0.061 / $0.039 | 2.72 / 3.56 |
| G05 | github-code-research | 9.0 / 9.0 | 61.2k / 32.7k | 38.3k / 30.7k | 22.0k / 13.7k | 3 / 3 | 18s / 21s | $0.077 / $0.061 | 1.47 / 2.75 |
| G06 | github-code-research | 7.0 / 8.0 | 42.6k / 25.6k | 29.8k / 20.7k | 13.1k / 6.6k | 4 / 3 | 14s / 18s | $0.060 / $0.041 | 1.65 / 3.12 |
| G07 | github-code-research | 10.0 / 10.0 | 45.2k / 25.7k | 36.5k / 28.0k | 15.8k / 11.4k | 3 / 3 | 19s / 20s | $0.073 / $0.056 | 2.21 / 3.89 |
| G08 | github-bug-rca | 9.0 / 8.0 | 34.4k / 19.3k | 19.3k / 17.9k | 5.0k / 5.1k | 2 / 2 | 10s / 16s | $0.039 / $0.036 | 2.62 / 4.14 |
| G09 | github-bug-rca | 9.0 / 8.0 | 42.2k / 23.0k | 35.0k / 24.5k | 12.9k / 8.7k | 2 / 2 | 14s / 17s | $0.070 / $0.049 | 2.13 / 3.48 |
| G10 | github-pr-review | 10.0 / 9.0 | 90.5k / 41.6k | 75.1k / 38.3k | 51.3k / 17.7k | 5 / 4 | 24s / 23s | $0.150 / $0.077 | 1.10 / 2.16 |
| G11 | github-structure | 8.0 / 7.5 | 31.8k / 16.9k | 15.4k / 14.0k | 2.3k / 2.5k | 2 / 2 | 13s / 14s | $0.031 / $0.028 | 2.51 / 4.44 |
| G12 | github-structure | 9.5 / 10.0 | 34.1k / 18.2k | 22.4k / 18.6k | 4.6k / 3.9k | 3 / 2 | 15s / 17s | $0.045 / $0.037 | 2.79 / 5.50 |
| G13 | github-repo-discovery | 10.0 / 10.0 | 43.7k / 21.6k | 17.9k / 12.9k | 4.4k / 2.4k | 3 / 3 | 20s / 10s | $0.036 / $0.026 | 2.29 / 4.63 |
| G14 | github-repo-discovery | 10.0 / 10.0 | 22.0k / 24.2k | 14.4k / 17.2k | 2.4k / 5.0k | 2 / 3 | 9s / 17s | $0.029 / $0.034 | 4.54 / 4.14 |
| G15 | github-history | 8.5 / 7.0 | 23.6k / 11.2k | 17.8k / 12.0k | 3.9k / 1.6k | 1 / 1 | 15s / 9s | $0.036 / $0.024 | 3.61 / 6.27 |
| G16 | github-history | 10.0 / 10.0 | 50.8k / 24.7k | 24.5k / 17.0k | 11.5k / 5.5k | 3 / 3 | 15s / 19s | $0.049 / $0.034 | 1.97 / 4.05 |
| G17 | artifact | 10.0 / 9.0 | 35.2k / 22.7k | 21.3k / 21.7k | 5.8k / 8.3k | 3 / 2 | 15s / 16s | $0.043 / $0.043 | 2.84 / 3.96 |
| G18 | artifact | 10.0 / 10.0 | 34.3k / 27.8k | 20.3k / 21.7k | 4.8k / 8.6k | 3 / 4 | 14s / 15s | $0.041 / $0.043 | 2.92 / 3.60 |

## Tool usage

- **octocode**: ghGetFileContent 17, ghGetHistoryItem 16, ghSearchCode 8, ghStructure 2, ghSearchHistory 2, artifactSearch 2 · counters: clasify calls 0, matchString queries 15, lspSearch calls 0, astSearch calls 0 · tool errors 0 · permission denials 0
- **rg-gh**: Bash:invocation 49 · tool errors 3 · permission denials 0

Per-question counters (non-zero):

- G01: octocode.matchString queries=1
- G05: octocode.matchString queries=2
- G06: octocode.matchString queries=2
- G07: octocode.matchString queries=2
- G13: octocode.matchString queries=3
- G14: octocode.matchString queries=1
- G17: octocode.matchString queries=2
- G18: octocode.matchString queries=2

## Judge agreement

18 question-pairs judged; 0 needed a tie-break; 0 grader errors. Order-swap spread per worker score: mean 0.08, max 1; 36/36 within 1 point, 36/36 within 2. Preferred answer consistent across both orders: 15/18.

Reference issues raised by the judge:

- G04: Line numbers are ~3 lines off from the fetched file (e.g. Argument check at 3883, Option normalized check at 3419), within tolerance.
- G10: Omits that the PR also removes the getaddrinfo ignore in net_lookup_host.rs, and that tokio/tests/buffered.rs (untouched) still has a 'No socket' Miri ignore on a TCP test.
- G10: The reference omits that the PR also removes the Miri ignore on net_lookup_host's resolve_dns, and that buffered.rs still carries a leftover 'No `socket` on miri' ignore after the merge.
- G16: Key fact 5 names the label `__unix_socket__`, but the PR #19399 body calls it `__scrape_unix_socket__`. The constant's actual value at ea954809ce was not checked because the local search of scrape.go failed. This is minor and does not affect grading.
- G16: The reference names the label __unix_socket__ (UnixSocketLabel), but PR #19399's body uses __scrape_unix_socket__. I did not check the code at ea954809ce, so this is a possible naming discrepancy rather than a confirmed error. It does not affect grading.
- G17: The reference leaves out the empty 'security' extra, which is present in setup.py extras_require. The extras block is at lines 123-127.

## Wrong claims flagged

- G01 octocode: Span name is only the method, not 'METHOD /route' (wrong: _route_selected calls span.update_name(f"{span_method} {route}")) | Span name is only the method, not 'METHOD /route' — _route_selected calls span.update_name(f"{span_method} {route}") | Implies _operation wrappers add cost even when telemetry is off (they return a shared nullcontext; NativeTelemetry is bypassed) — minor
- G04 octocode: click.option("--0-file") with default expose_value=True raises TypeError, it does not just warn | click.option("--0-file") listed as warning; with default expose_value=True it still raises TypeError
- G04 rg-gh: click.Option(["FOO"]) raises 'No options defined' TypeError before the normalization warning | click.Option(["FOO"]) given as triggering normalized warning; it raises TypeError 'No options defined' before the check
- G06 octocode: Speculates get_environment_proxies likely lives outside httpx/_utils.py; it is at _utils.py:30 | Suggests get_environment_proxies is not in httpx/_utils.py and likely lives elsewhere; it is defined at _utils.py:30
- G10 rg-gh: net_lookup_host still excluded with 'No getaddrinfo' — the PR actually removes that ignore | net_lookup_host still excluded under Miri because of no getaddrinfo — the PR actually removes that ignore from resolve_dns
- G11 octocode: 'Three of the five work with only one client type' — actually four do
- G11 rg-gh: Intro says 'Two work with both clients' — only MockTransport works with both; contradicts its own table | 'Two work with both clients' — only MockTransport works with both; contradicts own table
- G12 octocode: Says __init__.py re-exports 14 names; it re-exports 15 (and the answer lists 15)
- G15 rg-gh: Calls #8067 a PR that was never merged; it is an open tracking issue | #8067 described as a PR that was never merged; it is an open tracking issue
- G17 rg-gh: Claims read errors are never retried even when retries are enabled; a non-default max_retries uses Retry.from_int, which does retry reads | Read errors are never retried even if retries are enabled with Retry defaults (Retry.from_int(n) leaves read=None, so reads can be retried)

## Cost

Worker research $2.12 (octocode $1.19, rg-gh $0.94) · judge $4.44 · reflections $1.82 · probes $0.00 · reflection synthesis $0.00 · reported Claude total $8.38. Classification provider cost and total system cost remain unknown.

## Isolation

Every run passed its profile isolation check (offered/called tools, MCP servers, no skills, no benchmark-file access).
