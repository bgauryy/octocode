# Unified benchmark — run `full-1`

Workers: `octocode`, `rg-gh` · model sonnet · 30 questions × 1 pass · judge Opus (blinded X/Y, both orders, tie-break when spread > 2).
Build: MCP dist sha256 `9ab5b3c8e31a` · Claude Code 2.1.285 (Claude Code).

Quality 0–10 = correctness 0–5 + completeness 0–3 + evidence 0–2. Tokens: total = all input kinds + output; research = total − (first-request context × requests). Efficiency = quality per 10k total tokens.

## Totals

| worker | mean quality | median quality | total tokens | research tokens | output tokens | requests | tool calls | cost | time | efficiency |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| octocode | 8.42 | 8.5 | 5697.5k | 424.6k | 19.1k | 129 | 117 | $2.96 | 10.4 min | 0.44 |
| rg-gh | 9.12 | 9.0 | 1049.6k | 457.7k | 23.8k | 116 | 92 | $1.86 | 9.0 min | 2.61 |

Per-question ratio octocode/rg-gh: total tokens mean 6.08× / median 6.55×; research tokens mean 1.09× / median 0.97×; cost mean 1.67× / median 1.64×. Quality delta (octocode − rg-gh): mean -0.70, wins/ties/losses 3/11/16.

## Fixed overhead ("reply OK" probe)

| worker | first-request context | tools offered | isolation probe |
|---|--:|--:|---|
| octocode | 40.8k | 13 | pass |
| rg-gh | 5.0k | 1 | pass |

## By category

| category | n | octocode quality | octocode tokens | octocode research | rg-gh quality | rg-gh tokens | rg-gh research |
|---|--:|--:|--:|--:|--:|--:|--:|
| github | 10 | 8.40 | 146.8k | 12.1k | 8.70 | 40.0k | 20.3k |
| local | 20 | 8.43 | 211.5k | 15.2k | 9.32 | 32.5k | 12.7k |
| github-bug-rca | 2 | 9.25 | 152.3k | 9.6k | 8.75 | 25.2k | 7.6k |
| github-code-research | 3 | 8.00 | 157.5k | 7.9k | 9.33 | 35.2k | 11.7k |
| github-pr-review | 5 | 8.30 | 138.2k | 15.7k | 8.30 | 48.7k | 30.5k |
| local-impact | 2 | 9.00 | 218.9k | 14.3k | 9.50 | 30.9k | 12.9k |
| local-locate | 1 | 8.50 | 167.4k | 3.9k | 9.00 | 18.5k | 3.1k |
| local-semantic | 6 | 8.75 | 211.1k | 20.3k | 9.67 | 33.6k | 13.9k |
| local-trace | 11 | 8.14 | 214.3k | 13.6k | 9.14 | 33.5k | 12.9k |

Token columns are per-question means.

## Per question

Each cell shows octocode / rg-gh.

| q | category | quality | total tokens | research tokens | tool calls | time | cost | efficiency |
|---|---|--:|--:|--:|--:|--:|--:|--:|
| G01 | github-pr-review | 7.0 / 8.5 | 137.5k / 102.7k | 15.0k / 77.4k | 3 / 6 | 28s / 31s | $0.121 / $0.146 | 0.51 / 0.83 |
| G02 | github-pr-review | 8.0 / 8.5 | 137.5k / 54.0k | 15.0k / 28.8k | 3 / 4 | 22s / 21s | $0.113 / $0.094 | 0.58 / 1.57 |
| G03 | github-pr-review | 9.0 / 9.0 | 89.8k / 18.5k | 8.2k / 8.4k | 1 / 1 | 14s / 13s | $0.072 / $0.056 | 1.00 / 4.87 |
| G04 | github-pr-review | 8.0 / 7.0 | 128.8k / 22.1k | 6.5k / 7.0k | 2 / 2 | 16s / 14s | $0.066 / $0.048 | 0.62 / 3.17 |
| G05 | github-code-research | 8.0 / 9.0 | 257.0k / 37.9k | 12.2k / 12.7k | 5 / 5 | 24s / 20s | $0.112 / $0.061 | 0.31 / 2.37 |
| G06 | github-code-research | 8.0 / 9.0 | 129.3k / 38.2k | 6.9k / 13.0k | 2 / 5 | 16s / 25s | $0.067 / $0.058 | 0.62 / 2.36 |
| G07 | github-code-research | 8.0 / 10.0 | 86.1k / 29.5k | 4.5k / 9.3k | 2 / 3 | 21s / 23s | $0.072 / $0.062 | 0.93 / 3.39 |
| G08 | github-bug-rca | 10.0 / 10.0 | 128.1k / 19.2k | 5.8k / 4.2k | 3 / 2 | 15s / 16s | $0.064 / $0.038 | 0.78 / 5.20 |
| G09 | github-bug-rca | 8.5 / 7.5 | 176.5k / 31.2k | 13.4k / 11.1k | 3 / 3 | 19s / 20s | $0.094 / $0.059 | 0.48 / 2.40 |
| G10 | github-pr-review | 9.5 / 8.5 | 197.2k / 46.2k | 34.0k / 31.1k | 3 / 2 | 31s / 21s | $0.144 / $0.106 | 0.48 / 1.84 |
| L01 | local-trace | 9.0 / 10.0 | 167.9k / 20.1k | 4.4k / 4.7k | 3 / 2 | 19s / 14s | $0.083 / $0.040 | 0.54 / 4.98 |
| L02 | local-locate | 8.5 / 9.0 | 167.4k / 18.5k | 3.9k / 3.1k | 4 / 2 | 13s / 11s | $0.069 / $0.033 | 0.51 / 4.87 |
| L03 | local-trace | 9.0 / 10.0 | 263.5k / 40.1k | 18.2k / 14.5k | 7 / 4 | 22s / 18s | $0.118 / $0.059 | 0.34 / 2.49 |
| L04 | local-trace | 6.5 / 9.0 | 329.9k / 55.2k | 43.5k / 29.5k | 6 / 5 | 23s / 19s | $0.154 / $0.085 | 0.20 / 1.63 |
| L05 | local-impact | 8.0 / 9.0 | 171.9k / 19.3k | 8.2k / 3.9k | 5 / 2 | 23s / 12s | $0.083 / $0.035 | 0.47 / 4.65 |
| L06 | local-trace | 8.5 / 9.0 | 172.2k / 20.6k | 8.7k / 5.2k | 4 / 2 | 20s / 13s | $0.085 / $0.039 | 0.49 / 4.37 |
| L07 | local-trace | 7.5 / 9.5 | 173.5k / 25.6k | 9.9k / 10.2k | 5 / 2 | 18s / 16s | $0.090 / $0.058 | 0.43 / 3.71 |
| L08 | local-trace | 8.0 / 9.0 | 215.4k / 41.0k | 11.0k / 15.4k | 4 / 4 | 20s / 16s | $0.095 / $0.057 | 0.37 / 2.20 |
| L09 | local-semantic | 7.5 / 10.0 | 393.4k / 59.6k | 66.2k / 28.8k | 7 / 5 | 37s / 23s | $0.189 / $0.083 | 0.19 / 1.68 |
| L10 | local-semantic | 9.0 / 10.0 | 173.1k / 24.8k | 9.5k / 9.4k | 3 / 2 | 14s / 15s | $0.092 / $0.057 | 0.52 / 4.03 |
| L11 | local-trace | 9.0 / 10.0 | 219.5k / 34.3k | 15.0k / 13.8k | 4 / 4 | 20s / 19s | $0.104 / $0.064 | 0.41 / 2.91 |
| L12 | local-semantic | 9.0 / 9.0 | 223.2k / 22.0k | 18.6k / 6.6k | 5 / 2 | 21s / 18s | $0.116 / $0.048 | 0.40 / 4.09 |
| L13 | local-trace | 6.0 / 8.0 | 176.1k / 45.5k | 12.6k / 19.9k | 3 / 4 | 20s / 22s | $0.102 / $0.071 | 0.34 / 1.76 |
| L14 | local-semantic | 9.0 / 9.0 | 126.0k / 18.8k | 3.3k / 3.4k | 2 / 2 | 16s / 15s | $0.062 / $0.039 | 0.71 / 4.78 |
| L15 | local-semantic | 10.0 / 10.0 | 220.2k / 33.7k | 15.7k / 13.2k | 4 / 3 | 23s / 17s | $0.105 / $0.057 | 0.45 / 2.97 |
| L16 | local-trace | 8.0 / 7.5 | 170.9k / 23.9k | 7.3k / 8.5k | 4 / 2 | 18s / 15s | $0.087 / $0.052 | 0.47 / 3.14 |
| L17 | local-trace | 9.0 / 8.5 | 212.2k / 27.9k | 7.7k / 7.4k | 4 / 3 | 17s / 17s | $0.088 / $0.050 | 0.42 / 3.04 |
| L18 | local-semantic | 8.0 / 10.0 | 130.8k / 42.7k | 8.1k / 22.1k | 3 / 3 | 16s / 17s | $0.084 / $0.071 | 0.61 / 2.34 |
| L19 | local-trace | 9.0 / 10.0 | 256.4k / 33.9k | 10.9k / 13.3k | 5 / 3 | 21s / 20s | $0.110 / $0.059 | 0.35 / 2.95 |
| L20 | local-impact | 10.0 / 10.0 | 265.9k / 42.5k | 20.5k / 21.9k | 8 / 3 | 36s / 20s | $0.122 / $0.081 | 0.38 / 2.36 |

## Tool usage

- **octocode**: localSearch 47, localFetch 40, ghGetHistoryItem 14, ghGetFileContent 11, structureSearch 3, ghSearchHistory 2 · counters: clasify calls 0, matchString queries 16, lspSearch calls 0, astSearch calls 0, followUp uses 0 · tool errors 34 · permission denials 0
- **rg-gh**: Bash:cd 54, Bash:sed 19, Bash:gh 15, Bash:grep 2, Bash:ls 1, Bash:mkdir 1 · tool errors 0 · permission denials 0

Per-question counters (non-zero):

- G05: octocode.matchString queries=4
- G06: octocode.matchString queries=1
- G07: octocode.matchString queries=1
- G10: octocode.matchString queries=2
- L04: octocode.matchString queries=1
- L09: octocode.matchString queries=3
- L13: octocode.matchString queries=2
- L17: octocode.matchString queries=2

## Judge agreement

30 question-pairs judged; 0 needed a tie-break; 0 grader errors. Order-swap spread per worker score: mean 0.27, max 2; 59/60 within 1 point, 60/60 within 2. Preferred answer consistent across both orders: 27/30.

Reference issues raised by the judge:

- G10: The reference leaves out the additional tcp_socket.rs Miri ignores (keepalive, reuseaddr read, reuseport, send/recv buffer size) and the removal of net_lookup_host's getaddrinfo ignore; both answers correctly include the tcp_socket ignores.
- L07: Minor: the reference says prerenderToStream (app-render.tsx:9839-9846) uses the 'same pattern' as the page-render catch, but that copy does not append mutable cookies.
- L20: The reference calls the four call sites the full non-test set, but showconfig.go:47 also uses GetResolveJsonModule as a method value, which affects --showConfig output.

## Wrong claims flagged

- G01 rg-gh: Implies every app pays full per-request wrapping even without SDK; NativeTelemetry is bypassed when enabled() is false (hedged)
- G03 rg-gh: Minor: says tests cover 'three cases' while listing four
- G04 rg-gh: Says click.option('--x','Foo_Bar') currently names foo_bar; in 8.x the explicit name is kept as given and only 9.0 will lower-case it | States click.option('--x','Foo_Bar') currently names foo_bar; in 8.x the explicit name is kept as given, and 9.0 will lower-case it
- G07 octocode: User middleware described as 'in the order given' without noting add_middleware insert(0) semantics (imprecise)
- G09 rg-gh: Merge commit SHA garbled as 5da36b5de4f44a572ca5c12104fd2f8669fd feca (actual ...8669dfeaca) | Merge commit SHA quoted as '5da36b5de4f44a572ca5c12104fd2f8669fd feca'; the actual SHA is 5da36b5de4f44a572ca5c12104fd2f8669dfeaca
- G10 rg-gh: SO_LINGER ignored in 'three places' in tcp_socket.rs (actually two: basic_linger and linger) | SO_LINGER ignored in three places in tcp_socket.rs (actually two: basic_linger and the linger macro test)
- L04 octocode: Lists self/cls stripping as applying on this path | Says the invalid-docstring error needs both no Args: block AND too few blocks; the code uses OR | Cites base.py:~355-361 for the _create_subset_model return; it is actually at 362-368
- L04 rg-gh: Lists self/cls stripping for methods among the drops, but on the from_function path filter_args is always non-empty, so that step doesn't run | Claims self/cls are dropped for methods on this path; it doesn't happen because filter_args is always non-empty from from_function
- L05 octocode: Says 'the other four callers' but then lists five | Files the test under 'would break' although the call works with one other content (nuance given afterwards) | Says 'other four callers' but lists five unaffected sites
- L06 octocode: Labels the base-server.ts:2152 function as 'sendResponse' (unverified/likely misnamed; minor)
- L09 octocode: Attributes the comment near line 1980 to relabel-dropped series; it actually concerns new series not appended due to sample_limit/errors | Claims series dropped by relabeling are deliberately not tracked, citing the comment near line 1980; that comment is about new series not appended due to sample_limit/errors
- L11 octocode: harness.rs:508 described as the path for a future that panicked during poll; it is cancel_task (cancellation). The poll panic mapping is panic_to_error at harness.rs:546 | harness.rs:508 described as the panic path for a panicking future; it is cancel_task (cancellation path); poll-time panic mapping is at harness.rs:546
- L12 rg-gh: Says the lifo slot task is moved to the run queue 'when a worker parks or shuts down'; the cited code (worker.rs:478-484) is in block_in_place and handles handing off the core | Says the LIFO slot task is moved to the run queue when a worker parks or shuts down; the code at :478-483 is in the block_in_place core hand-off | Notify description is slightly overbroad (the non-LIFO push also notifies)
- L13 octocode: Says the propagated deletion is a DEL; it is UNLINK when lazyfree-lazy-expire is enabled | t_string.c line range 468-478 is off (getGenericCommand/getCommand are at 456-472) | Says the master propagates a DEL; it is UNLINK when lazyfree-lazy-expire is enabled
- L15 rg-gh: Cites line 665 for the 'built-in flooding protection' comment; that text is at line 884 (minor citation slip)
- L16 octocode: Minor citation drift: initial-capacity clamp cited at :280-282 (actually ~275-278); initTable cited at 1976-1979 (actually ~1998-2004)
- L16 rg-gh: Example: maximumSize(100), concurrencyLevel(16) gives 4 segments; the loop actually reaches 8 (4*20=80<=100 allows doubling to 8; it stops at 8 because 160>100) | Example: maximumSize(100) + concurrencyLevel(16) gives 4 segments — actually 8 (4*20=80<=100 allows doubling to 8; stops there since 160>100)

## Cost

Workers $4.83 (octocode $2.96, rg-gh $1.86) · judge $7.37 · total $12.20.

## Isolation

Every run passed its profile isolation check (offered/called tools, MCP servers, no skills, no benchmark-file access).
