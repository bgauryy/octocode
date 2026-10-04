# Unified benchmark — run `s12fix-p2`

Workers: `octocode`, `rg-gh` · model claude-sonnet-5-5 · 18 questions × 1 pass · judge claude-opus-5-5 (blinded X/Y, both orders, tie-break when spread > 2).
Build: MCP dist sha256 `39291b81b057` · Claude Code 2.1.288 (Claude Code).

Quality 0–10 = correctness 0–5 + completeness 0–3 + evidence 0–2. Tokens: total = all input kinds + output; research is an estimate: total − (first-request context × requests). Weighted tokens are input-token equivalents under the frozen tariff claude-sonnet-5-5@tri-20261002 ($2/M input, $4/M 1h cache write, $0.2/M cache read, $10/M output); "cold" writes each session's first cached prefix instead of reading it. Q/$ = summed quality per dollar. Cost and tokens cover the worker Claude session; classifier provider totals are separate/unknown. Efficiency = quality per 10k total tokens; weighted efficiency = quality per 10k weighted tokens.

## Totals

| worker | mean quality | median quality | total tokens | weighted tokens (warm / cold) | research tokens | output tokens | requests | tool calls | cost | time | efficiency | weighted efficiency (warm / cold) | Q/$ (warm / cold) |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| octocode | 8.86 | 9.0 | 809.5k | 554.7k / 792.8k | 259.9k | 26.3k | 56 | 44 | $1.11 | 4.9 min | 1.97 | 2.88 / 2.01 | 143.8 / 100.6 |
| rg-gh | 8.86 | 9.0 | 500.2k | 445.3k / 509.9k | 194.9k | 26.9k | 64 | 52 | $0.89 | 5.3 min | 3.19 | 3.58 / 3.13 | 179.1 / 156.4 |

Per-question ratio octocode/rg-gh: total tokens mean 1.81× / median 1.59×; weighted tokens mean 1.28× / median 1.19×; research tokens mean 1.63× / median 1.25×; cost mean 1.28× / median 1.19×. Quality delta (octocode − rg-gh): mean 0.00, wins/ties/losses 4/11/3.

## Fixed overhead ("reply OK" probe)

| worker | first-request context | tools offered | isolation probe |
|---|--:|--:|---|
| octocode | — | — | not run |
| rg-gh | — | — | not run |

## By category

| category | n | octocode quality | octocode tokens | octocode research | rg-gh quality | rg-gh tokens | rg-gh research |
|---|--:|--:|--:|--:|--:|--:|--:|
| github | 18 | 8.86 | 45.0k | 14.4k | 8.86 | 27.8k | 10.8k |
| local | 0 | — | — | — | — | — | — |
| artifact | 2 | 10.00 | 35.0k | 5.5k | 9.75 | 26.9k | 7.8k |
| github-bug-rca | 2 | 8.00 | 38.0k | 8.6k | 8.50 | 20.4k | 6.2k |
| github-code-research | 3 | 9.33 | 42.7k | 13.3k | 9.00 | 39.9k | 17.7k |
| github-history | 2 | 10.00 | 35.9k | 6.4k | 8.50 | 14.1k | 2.1k |
| github-pr-review | 5 | 7.50 | 63.7k | 32.3k | 8.50 | 36.2k | 19.0k |
| github-repo-discovery | 2 | 10.00 | 38.8k | 4.4k | 9.75 | 20.9k | 4.1k |
| github-structure | 2 | 9.00 | 33.8k | 4.3k | 8.50 | 17.4k | 3.0k |

Token columns are per-question means.

## Per question

Each cell shows octocode / rg-gh.

| q | category | quality | total tokens | weighted tokens | research tokens | tool calls | time | cost | efficiency |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|
| G01 | github-pr-review | 8.0 / 9.0 | 120.8k / 76.0k | 79.4k / 62.7k | 71.7k / 47.4k | 4 / 7 | 31s / 43s | $0.159 / $0.125 | 0.66 / 1.18 |
| G02 | github-pr-review | 8.5 / 8.0 | 53.9k / 40.9k | 45.1k / 38.0k | 24.5k / 21.8k | 2 / 3 | 16s / 21s | $0.090 / $0.076 | 1.58 / 1.96 |
| G03 | github-pr-review | 7.5 / 8.0 | 28.5k / 18.0k | 28.8k / 27.0k | 8.9k / 8.5k | 1 / 1 | 15s / 13s | $0.058 / $0.054 | 2.63 / 4.45 |
| G04 | github-pr-review | 7.5 / 7.5 | 29.2k / 20.0k | 29.5k / 20.1k | 9.6k / 5.8k | 1 / 2 | 10s / 15s | $0.059 / $0.040 | 2.57 / 3.74 |
| G05 | github-code-research | 9.0 / 9.0 | 44.3k / 56.0k | 36.3k / 37.3k | 14.9k / 27.5k | 3 / 6 | 18s / 26s | $0.073 / $0.075 | 2.03 / 1.61 |
| G06 | github-code-research | 9.0 / 8.0 | 38.9k / 24.9k | 28.8k / 19.6k | 9.5k / 5.9k | 2 / 3 | 16s / 18s | $0.058 / $0.039 | 2.31 / 3.21 |
| G07 | github-code-research | 10.0 / 10.0 | 44.7k / 38.8k | 35.0k / 35.1k | 15.3k / 19.7k | 2 / 4 | 20s / 25s | $0.070 / $0.070 | 2.23 / 2.58 |
| G08 | github-bug-rca | 7.0 / 9.0 | 33.9k / 19.4k | 18.3k / 18.0k | 4.5k / 5.2k | 2 / 2 | 12s / 15s | $0.037 / $0.036 | 2.07 / 4.63 |
| G09 | github-bug-rca | 9.0 / 8.0 | 42.0k / 21.4k | 33.9k / 21.3k | 12.7k / 7.2k | 2 / 2 | 12s / 16s | $0.068 / $0.043 | 2.14 / 3.74 |
| G10 | github-pr-review | 6.0 / 10.0 | 86.2k / 26.0k | 58.1k / 33.2k | 46.9k / 11.7k | 3 / 2 | 33s / 17s | $0.116 / $0.066 | 0.70 / 3.84 |
| G11 | github-structure | 9.0 / 9.0 | 33.5k / 11.3k | 18.4k / 12.4k | 4.0k / 1.7k | 3 / 1 | 12s / 11s | $0.037 / $0.025 | 2.69 / 7.98 |
| G12 | github-structure | 9.0 / 8.0 | 34.2k / 23.5k | 22.7k / 18.9k | 4.7k / 4.4k | 3 / 3 | 17s / 15s | $0.045 / $0.038 | 2.64 / 3.40 |
| G13 | github-repo-discovery | 10.0 / 10.0 | 44.8k / 23.2k | 19.4k / 16.6k | 5.5k / 4.0k | 4 / 4 | 13s / 15s | $0.039 / $0.033 | 2.23 / 4.32 |
| G14 | github-repo-discovery | 10.0 / 9.5 | 32.7k / 18.6k | 16.4k / 16.1k | 3.2k / 4.3k | 2 / 3 | 13s / 12s | $0.033 / $0.032 | 3.06 / 5.10 |
| G15 | github-history | 10.0 / 7.0 | 37.2k / 10.9k | 22.9k / 11.3k | 7.7k / 1.3k | 2 / 1 | 16s / 10s | $0.046 / $0.023 | 2.69 / 6.39 |
| G16 | github-history | 10.0 / 10.0 | 34.7k / 17.3k | 18.9k / 14.8k | 5.2k / 2.9k | 2 / 2 | 13s / 13s | $0.038 / $0.030 | 2.89 / 5.77 |
| G17 | artifact | 10.0 / 9.5 | 24.9k / 21.7k | 21.9k / 20.5k | 5.2k / 7.3k | 2 / 2 | 12s / 15s | $0.044 / $0.041 | 4.02 / 4.38 |
| G18 | artifact | 10.0 / 10.0 | 45.1k / 32.2k | 21.0k / 22.3k | 5.7k / 8.2k | 4 / 4 | 15s / 18s | $0.042 / $0.045 | 2.22 / 3.11 |

## Tool usage

- **octocode**: ghGetFileContent 15, ghGetHistoryItem 14, ghSearchCode 9, ghStructure 2, ghSearchHistory 2, artifactSearch 2 · counters: clasify calls 0, matchString queries 16, lspSearch calls 0, astSearch calls 0 · tool errors 2 · permission denials 0
- **rg-gh**: Bash:invocation 52 · tool errors 4 · permission denials 0

Per-question counters (non-zero):

- G01: octocode.matchString queries=3
- G05: octocode.matchString queries=2
- G06: octocode.matchString queries=2
- G07: octocode.matchString queries=3
- G13: octocode.matchString queries=3
- G14: octocode.matchString queries=1
- G17: octocode.matchString queries=1
- G18: octocode.matchString queries=1

## Judge agreement

18 question-pairs judged; 0 needed a tie-break; 0 grader errors. Order-swap spread per worker score: mean 0.28, max 2; 34/36 within 1 point, 36/36 within 2. Preferred answer consistent across both orders: 10/18.

Reference issues raised by the judge:

- G16: Reference names the label `__unix_socket__`, but PR #19399 body uses `__scrape_unix_socket__`; I couldn't verify the source tree at ea954809ce.
- G16: The reference names the label `__unix_socket__` (via UnixSocketLabel), but the body of PR #19399 calls it `__scrape_unix_socket__`. I could not check the source at ea954809ce because rg was unavailable. This is minor and does not affect grading.
- G17: The reference's extras list omits the `security: []` extra, which setup.py:124 at v2.32.3 declares. Both answers correctly include it.
- G17: Reference omits the empty 'security' extra, which is present in setup.py extras_require at v2.32.3 (line 124).

## Wrong claims flagged

- G01 rg-gh: Implies middleware stack is always built eagerly in __call__; it is only in the telemetry-enabled branch (minor) | Slight overstatement that eager middleware build/scope['app'] is a telemetry-unrelated change (it only runs on the telemetry-enabled path)
- G03 octocode: States that on clientTtl eviction the code only spliced the client and did not close it, and that the same queue-pickup bug happened there; the old kRemoveClient did call client.close() | Claims clientTtl eviction previously only spliced the client out of kClients without closing it; the old kRemoveClient already called client.close(), and the bug there was only that the pool stopped t
- G03 rg-gh: Short answer implies clientTtl-evicted clients were removed without closing (old kRemoveClient already closed them; they were just untracked) | Summary implies clientTtl-evicted clients were removed without being closed (they were closed but untracked) | Wording implies the old 'cannot be closed in this state' comment was in round-robin-pool.js too; it was only in pool.js
- G04 octocode: click.option("--0-file") given as a warning example; with the value exposed (the default) a non-identifier derived option name still raises TypeError (core.py:3398-3407) | Gives click.option("--0-file") as a warning example; with exposed value it raises TypeError, not a warning
- G04 rg-gh: Says a derived non-identifier option name like --0-file warns; with the value exposed it raises TypeError, and it only warns when expose_value=False | Implies derived non-identifier option name like --0-file warns; with expose_value=True it still raises TypeError (only warns when expose_value=False)
- G08 octocode: Says the old trap 'didn't register any reactive dependency'; it did call get(s) when a data descriptor and a source both existed | Says the old code didn't read the source in the deleted case; it read it, only untracked via source?.v | Overstates that the old getOwnPropertyDescriptor trap registered no dependency at all / Object.hasOwn 'read nothing' — it did call get(s) when a descriptor with value and a source existed
- G10 octocode: coop_budget.rs listed as a UDP exclusion even though the PR doesn't touch it (taken from the default branch) | Mixes in default-branch state (e.g. pidfd_spawnp, coop_budget) not from the PR; minor
- G12 rg-gh: Says the package has 'six modules' but lists seven | Says the package has 'six modules' although it lists seven
- G15 rg-gh: Describes #8067 as an open, unmerged re-land PR; it is an open tracking issue, and the re-land was merged in #8337 | #8067 described as an open PR / 're-land attempt'; it is an open tracking issue
- G17 rg-gh: Minor: classifiers list 'Python :: 3.8-3.12', not specifically 'CPython 3.8...'

## Cost

Worker research $2.00 (octocode $1.11, rg-gh $0.89) · judge $4.33 · reflections $1.78 · probes $0.00 · reflection synthesis $0.00 · reported Claude total $8.10. Classification provider cost and total system cost remain unknown.

## Isolation

Every run passed its profile isolation check (offered/called tools, MCP servers, no skills, no benchmark-file access).
