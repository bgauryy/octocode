# Unified benchmark — run `dedup-20260930`

Workers: `octocode`, `rg-gh` · model sonnet · 30 questions × 1 pass · judge Opus (blinded X/Y, both orders, tie-break when spread > 2).
Build: MCP dist sha256 `ed0164a47e3c` · Claude Code 2.1.286 (Claude Code).

Quality 0–10 = correctness 0–5 + completeness 0–3 + evidence 0–2. Tokens: total = all input kinds + output; research = total − (first-request context × requests). Weighted = input-token equivalents at Claude price multiples (cache write 1.25×, cache read 0.1×, output 5×), proportional to cost. Efficiency = quality per 10k total tokens; weighted efficiency = quality per 10k weighted tokens.

## Totals

| worker | mean quality | median quality | total tokens | weighted tokens | research tokens | output tokens | requests | tool calls | cost | time | efficiency | weighted efficiency |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| octocode | 8.38 | 8.5 | 2474.5k | 706.3k | 448.1k | 19.6k | 120 | 126 | $2.31 | 9.5 min | 1.02 | 3.56 |
| rg-gh | 9.18 | 9.0 | 1087.9k | 582.7k | 490.8k | 24.5k | 117 | 92 | $1.90 | 9.2 min | 2.53 | 4.73 |

Per-question ratio octocode/rg-gh: total tokens mean 2.53× / median 2.68×; weighted tokens mean 1.29× / median 1.23×; research tokens mean 1.16× / median 1.03×; cost mean 1.28× / median 1.28×. Quality delta (octocode − rg-gh): mean -0.80, wins/ties/losses 3/11/16.

## Fixed overhead ("reply OK" probe)

| worker | first-request context | tools offered | isolation probe |
|---|--:|--:|---|
| octocode | 16.8k | 13 | pass |
| rg-gh | 5.0k | 1 | pass |

## By category

| category | n | octocode quality | octocode tokens | octocode research | rg-gh quality | rg-gh tokens | rg-gh research |
|---|--:|--:|--:|--:|--:|--:|--:|
| github | 10 | 8.15 | 70.8k | 15.3k | 8.85 | 42.7k | 23.0k |
| local | 20 | 8.50 | 88.3k | 14.7k | 9.35 | 33.1k | 13.0k |
| github-bug-rca | 2 | 8.75 | 59.9k | 9.4k | 8.75 | 27.0k | 6.9k |
| github-code-research | 3 | 7.67 | 66.4k | 10.4k | 9.67 | 33.4k | 9.8k |
| github-pr-review | 5 | 8.20 | 77.9k | 20.7k | 8.40 | 54.5k | 37.3k |
| local-impact | 2 | 9.50 | 66.0k | 6.8k | 9.50 | 31.6k | 13.6k |
| local-locate | 1 | 9.00 | 72.4k | 4.7k | 9.00 | 23.6k | 3.2k |
| local-semantic | 6 | 8.08 | 101.4k | 19.7k | 9.50 | 35.0k | 14.4k |
| local-trace | 11 | 8.50 | 86.7k | 14.4k | 9.27 | 33.1k | 13.1k |

Token columns are per-question means.

## Per question

Each cell shows octocode / rg-gh.

| q | category | quality | total tokens | weighted tokens | research tokens | tool calls | time | cost | efficiency |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|
| G01 | github-pr-review | 7.0 / 9.0 | 128.9k / 110.3k | 39.9k / 48.1k | 44.7k / 85.1k | 4 / 6 | 35s / 31s | $0.151 / $0.165 | 0.54 / 0.82 |
| G02 | github-pr-review | 8.0 / 9.0 | 61.2k / 80.7k | 20.9k / 42.9k | 10.7k / 60.5k | 2 / 4 | 17s / 24s | $0.077 / $0.139 | 1.31 / 1.12 |
| G03 | github-pr-review | 9.0 / 8.0 | 41.9k / 18.5k | 20.2k / 17.9k | 8.2k / 8.4k | 1 / 1 | 14s / 13s | $0.061 / $0.058 | 2.15 / 4.33 |
| G04 | github-pr-review | 8.5 / 7.5 | 56.9k / 22.0k | 18.4k / 15.3k | 6.5k / 6.9k | 2 / 2 | 15s / 13s | $0.052 / $0.047 | 1.49 / 3.41 |
| G05 | github-code-research | 8.0 / 10.0 | 77.4k / 37.7k | 18.8k / 17.8k | 10.1k / 12.5k | 3 / 4 | 19s / 22s | $0.069 / $0.062 | 1.03 / 2.65 |
| G06 | github-code-research | 7.0 / 9.0 | 58.8k / 33.6k | 16.0k / 16.3k | 8.4k / 8.4k | 3 / 5 | 20s / 21s | $0.061 / $0.053 | 1.19 / 2.68 |
| G07 | github-code-research | 8.0 / 10.0 | 63.1k / 28.8k | 22.2k / 16.3k | 12.6k / 8.6k | 5 / 3 | 19s / 23s | $0.079 / $0.057 | 1.27 / 3.47 |
| G08 | github-bug-rca | 9.0 / 9.5 | 56.0k / 33.0k | 17.2k / 14.3k | 5.6k / 7.9k | 3 / 5 | 13s / 29s | $0.048 / $0.045 | 1.61 / 2.88 |
| G09 | github-bug-rca | 8.5 / 8.0 | 63.7k / 21.1k | 25.6k / 15.4k | 13.3k / 6.0k | 3 / 2 | 19s / 16s | $0.079 / $0.047 | 1.33 / 3.80 |
| G10 | github-pr-review | 8.5 / 8.5 | 100.5k / 41.0k | 38.7k / 28.3k | 33.2k / 25.8k | 3 / 2 | 24s / 19s | $0.122 / $0.091 | 0.85 / 2.08 |
| L01 | local-trace | 9.5 / 10.0 | 53.9k / 20.2k | 12.7k / 13.1k | 3.2k / 4.8k | 2 / 2 | 20s / 12s | $0.055 / $0.039 | 1.76 / 4.96 |
| L02 | local-locate | 9.0 / 9.0 | 72.4k / 23.6k | 18.0k / 11.8k | 4.7k / 3.2k | 4 / 3 | 17s / 15s | $0.053 / $0.035 | 1.24 / 3.81 |
| L03 | local-trace | 8.5 / 10.0 | 102.7k / 20.8k | 25.2k / 10.5k | 18.2k / 5.4k | 7 / 2 | 18s / 18s | $0.080 / $0.048 | 0.83 / 4.82 |
| L04 | local-trace | 8.0 / 9.0 | 123.2k / 36.7k | 39.9k / 20.5k | 38.6k / 16.1k | 8 / 3 | 25s / 19s | $0.137 / $0.073 | 0.65 / 2.45 |
| L05 | local-impact | 9.0 / 9.0 | 55.9k / 19.0k | 17.1k / 11.3k | 5.1k / 3.5k | 3 / 2 | 12s / 12s | $0.050 / $0.033 | 1.61 / 4.74 |
| L06 | local-trace | 8.0 / 9.0 | 56.4k / 31.9k | 17.7k / 16.6k | 5.7k / 11.3k | 4 / 3 | 14s / 18s | $0.054 / $0.050 | 1.42 / 2.83 |
| L07 | local-trace | 8.0 / 10.0 | 60.3k / 37.7k | 20.9k / 19.5k | 9.5k / 17.1k | 5 / 3 | 18s / 19s | $0.069 / $0.066 | 1.33 / 2.66 |
| L08 | local-trace | 8.5 / 8.0 | 75.2k / 26.6k | 17.6k / 18.1k | 7.6k / 11.3k | 3 / 2 | 15s / 13s | $0.063 / $0.055 | 1.13 / 3.00 |
| L09 | local-semantic | 7.0 / 10.0 | 210.3k / 72.1k | 45.6k / 24.8k | 58.1k / 36.2k | 9 / 6 | 32s / 24s | $0.143 / $0.083 | 0.33 / 1.39 |
| L10 | local-semantic | 9.0 / 9.0 | 58.7k / 21.6k | 21.4k / 15.4k | 8.0k / 6.2k | 3 / 2 | 15s / 14s | $0.067 / $0.049 | 1.53 / 4.16 |
| L11 | local-trace | 9.0 / 10.0 | 79.6k / 37.3k | 22.5k / 20.9k | 11.9k / 16.8k | 5 / 3 | 17s / 19s | $0.071 / $0.070 | 1.13 / 2.68 |
| L12 | local-semantic | 9.0 / 10.0 | 58.4k / 21.7k | 16.9k / 14.8k | 7.6k / 6.3k | 4 / 2 | 23s / 16s | $0.072 / $0.049 | 1.54 / 4.60 |
| L13 | local-trace | 7.0 / 9.0 | 99.1k / 60.9k | 26.2k / 25.5k | 14.5k / 30.1k | 4 / 5 | 16s / 21s | $0.074 / $0.078 | 0.71 / 1.48 |
| L14 | local-semantic | 8.0 / 9.0 | 71.9k / 19.1k | 18.2k / 12.7k | 4.2k / 3.7k | 3 / 2 | 14s / 15s | $0.053 / $0.039 | 1.11 / 4.72 |
| L15 | local-semantic | 8.5 / 9.0 | 100.8k / 35.9k | 26.3k / 19.1k | 16.3k / 15.4k | 4 / 3 | 16s / 18s | $0.077 / $0.063 | 0.84 / 2.50 |
| L16 | local-trace | 9.0 / 8.0 | 55.3k / 20.5k | 16.6k / 13.1k | 4.5k / 5.1k | 3 / 2 | 13s / 16s | $0.049 / $0.042 | 1.63 / 3.91 |
| L17 | local-trace | 9.0 / 10.0 | 146.7k / 38.6k | 32.5k / 17.9k | 28.3k / 12.9k | 9 / 4 | 25s / 19s | $0.104 / $0.059 | 0.61 / 2.59 |
| L18 | local-semantic | 7.0 / 10.0 | 108.3k / 39.2k | 28.3k / 19.8k | 23.7k / 18.7k | 6 / 3 | 23s / 17s | $0.090 / $0.064 | 0.65 / 2.55 |
| L19 | local-trace | 9.0 / 9.0 | 101.1k / 33.5k | 26.6k / 18.2k | 16.4k / 12.9k | 5 / 3 | 19s / 16s | $0.081 / $0.059 | 0.89 / 2.68 |
| L20 | local-impact | 10.0 / 10.0 | 76.2k / 44.3k | 18.2k / 26.3k | 8.5k / 23.7k | 6 / 3 | 24s / 19s | $0.072 / $0.084 | 1.31 / 2.26 |

## Tool usage

- **octocode**: localFetch 48, localSearch 47, ghGetHistoryItem 16, ghGetFileContent 10, ghSearchHistory 2, structureSearch 2, ghSearchCode 1 · counters: clasify calls 0, matchString queries 14, lspSearch calls 0, astSearch calls 0 · tool errors 12 · permission denials 0
- **rg-gh**: Bash:cd 63, Bash:gh 16, Bash:sed 11, Bash:find 1, Bash:f 1 · tool errors 2 · permission denials 0

Per-question counters (non-zero):

- G05: octocode.matchString queries=2
- G06: octocode.matchString queries=3
- G07: octocode.matchString queries=2
- G10: octocode.matchString queries=2
- L04: octocode.matchString queries=1
- L09: octocode.matchString queries=1
- L12: octocode.matchString queries=1
- L15: octocode.matchString queries=1
- L17: octocode.matchString queries=1

## Judge agreement

30 question-pairs judged; 1 needed a tie-break; 0 grader errors. Order-swap spread per worker score: mean 0.22, max 3; 59/60 within 1 point, 59/60 within 2. Preferred answer consistent across both orders: 26/30.

Reference issues raised by the judge:

- G10: The reference omits the tcp_socket.rs ignores added for the keepalive, reuseaddr, reuseport and send/recv buffer-size socket options. Both answers correctly report them.
- G10: The reference leaves out the tcp_socket.rs ignores for keepalive, reuseaddr, reuseport, send buffer size and receive buffer size, each with a 'Miri doesn't support ...' reason. It also leaves out the uds_cred.rs rewording to 'No getsockopt for Unix domain sockets'.
- L08: Minor: the reference's line cite for the staleness check (388-391) is correct; no substantive issues.
- L13: Reference omits the asmIsKeyInTrimJob/KEY_TRIMMED early-return in expireIfNeeded (db.c:3011-3019), which is present at this commit; minor.

## Wrong claims flagged

- G01 octocode: 'Only way to opt out is per app' slightly overstated (OTEL_SDK_DISABLED env also disables auto-configure) | Merge date stated as 2026-09-26; actual mergedAt is 2026-09-29
- G01 rg-gh: Minor: suggests new BackgroundTasks loop only awaiting task() may mishandle sync funcs; Starlette's BackgroundTask handles that
- G03 rg-gh: Says clientTtl-evicted clients were removed by a bare splice and never closed; the old kRemoveClient did call client.close(() => {}) but didn't track the client | Says clientTtl eviction now calls kRetireClient directly; it still calls kRemoveClient, which hands off to kRetireClient (imprecise) | States clientTtl-evicted clients were dropped by bare splice without closing (old kRemoveClient did close them via client.close(() => {}))
- G04 rg-gh: 'click.option("--x", "Foo_Bar") warns because it names foo_bar rather than Foo_Bar' — in 8.x the name is kept as Foo_Bar; only 9.0 will lower-case it (ambiguous/misleading)
- G09 octocode: Slightly imprecise: calls the mutated dict 'the user's own config dict' for models; the mutated dict is the class-level model_config (merged namespace dict)
- G10 octocode: Calls #7010 a 'Miri issue'; it is tokio#7010 (minor)
- G10 rg-gh: Describes tcp_stream readv exclusion as 'a test guarded by cfg(not(miri))' — it is a block inside try_read_write (minor imprecision)
- L03 octocode: Callbacks registered during hook execution 'go into the new list' — actually they run immediately since autocommit is already True and not in atomic block (minor)
- L04 octocode: Lists self/cls stripping among filtered args on this path (not applicable, since filter_args is non-empty) | Parser location given as function_calling.py:759-820 (starts at 735) | Implies self/cls stripping applies on the @tool path; it only runs when filter_args is empty, and from_function always passes non-empty filter_args
- L04 rg-gh: Lists self/cls stripping among filtered args on this path; from_function always passes non-empty filter_args so that branch is skipped | Lists self/cls among the filtered args without noting it only applies when filter_args is empty, which never happens on this path
- L06 octocode: config-shared.ts:2614 described as copying into render options; it is a serialized config object, not renderOpts (minor) | config-shared.ts:2614 described as copying into render options (it is a serialized config subset)
- L08 rg-gh: Speculates that the sampleLimitErr check at scrape.go:1988 gates the success path; it only gates the seriesAdded increment (hedged) | Hedged claim that the sampleLimitErr check at scrape.go:1988 gates the normal success path; it only gates the seriesAdded++ counter
- L09 octocode: forEachStale cited at scrape.go:1188-1196; actual location is 1167-1175 | forEachStale cited at scrape.go:1188-1196 (actually 1167-1175)
- L11 octocode: Cites harness.rs:508 for storing a poll panic as a JoinError; that line is cancel_task, and the poll-panic mapping is at harness.rs:546 | harness.rs:508 cited for panic storage is cancel_task (minor) | Cites harness.rs:508 for panic storage; the poll_future panic mapping is at 546 (508 is cancel_task) — minor citation imprecision
- L11 rg-gh: harness.rs:508 cited for panic storage is actually cancel_task; poll panic mapping is at 546 (minor)
- L14 octocode: Minor: implies call state is recorded before shouldInvoke is evaluated (actually isInvoking is computed first)
- L16 rg-gh: Size cap ensures each segment gets 'at least about 20 entries' (actually ~10) | Claims each segment gets at least about 20 entries of capacity; the loop checks before doubling, so the guarantee is about 10 (code comment at :281).
- L18 octocode: Cites lines 689-695 for the `*ref_stack.back() = discarded` assignment; it is actually at 661 (689-695 are asserts and pops) | end_object discard cited at lines 689-695 (actually line 661; 688-694 are asserts/pops)

## Cost

Workers $4.21 (octocode $2.31, rg-gh $1.90) · judge $7.49 · total $11.70.

## Isolation

Every run passed its profile isolation check (offered/called tools, MCP servers, no skills, no benchmark-file access).
