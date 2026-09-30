# Unified benchmark — run `smoke-1`

Workers: `octocode`, `rg-gh` · model sonnet · 2 questions × 1 pass · judge Opus (blinded X/Y, both orders, tie-break when spread > 2).
Build: MCP dist sha256 `9ab5b3c8e31a` · Claude Code 2.1.285 (Claude Code).

Quality 0–10 = correctness 0–5 + completeness 0–3 + evidence 0–2. Tokens: total = all input kinds + output; research = total − (first-request context × requests). Efficiency = quality per 10k total tokens.

## Totals

| worker | mean quality | median quality | total tokens | research tokens | output tokens | requests | tool calls | cost | time | efficiency |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| octocode | 8.25 | 8.3 | 257.5k | 12.3k | 1.6k | 6 | 4 | $0.14 | 0.5 min | 0.64 |
| rg-gh | 9.00 | 9.0 | 37.5k | 12.0k | 1.6k | 5 | 3 | $0.10 | 0.4 min | 4.80 |

Per-question ratio octocode/rg-gh: total tokens mean 6.84× / median 6.84×; research tokens mean 1.06× / median 1.06×; cost mean 1.54× / median 1.54×. Quality delta (octocode − rg-gh): mean -0.75, wins/ties/losses 0/1/1.

## Fixed overhead ("reply OK" probe)

| worker | first-request context | tools offered | isolation probe |
|---|--:|--:|---|
| octocode | 40.8k | 13 | pass |
| rg-gh | 5.0k | 1 | not run |

## By category

| category | n | octocode quality | octocode tokens | octocode research | rg-gh quality | rg-gh tokens | rg-gh research |
|---|--:|--:|--:|--:|--:|--:|--:|
| github | 1 | 9.00 | 89.8k | 8.2k | 9.00 | 18.5k | 8.4k |
| local | 1 | 7.50 | 167.7k | 4.1k | 9.00 | 19.0k | 3.6k |
| github-pr-review | 1 | 9.00 | 89.8k | 8.2k | 9.00 | 18.5k | 8.4k |
| local-semantic | 1 | 7.50 | 167.7k | 4.1k | 9.00 | 19.0k | 3.6k |

Token columns are per-question means.

## Per question

Each cell shows octocode / rg-gh.

| q | category | quality | total tokens | research tokens | tool calls | time | cost | efficiency |
|---|---|--:|--:|--:|--:|--:|--:|--:|
| G03 | github-pr-review | 9.0 / 9.0 | 89.8k / 18.5k | 8.2k / 8.4k | 1 / 1 | 12s / 13s | $0.071 / $0.057 | 1.00 / 4.87 |
| L14 | local-semantic | 7.5 / 9.0 | 167.7k / 19.0k | 4.1k / 3.6k | 3 / 2 | 15s / 13s | $0.071 / $0.039 | 0.45 / 4.74 |

## Tool usage

- **octocode**: localFetch 2, ghGetHistoryItem 1, localSearch 1 · counters: clasify calls 0, matchString queries 0, lspSearch calls 0, astSearch calls 0, followUp uses 0 · tool errors 1 · permission denials 0
- **rg-gh**: Bash:gh 1, Bash:cd 1, Bash:sed 1 · tool errors 0 · permission denials 0

## Judge agreement

2 question-pairs judged; 0 needed a tie-break; 0 grader errors. Order-swap spread per worker score: mean 0.25, max 1; 4/4 within 1 point, 4/4 within 2. Preferred answer consistent across both orders: 1/2.

Reference issues raised by the judge:

- G03: Minor: the reference quotes only the first line of the removed pool.js comment; it continues 'but the client cannot be closed in this state'. Not an error.

## Wrong claims flagged

- L14 octocode: Describes the maxing immediate-invoke branch as running whenever a timer is already running and maxing is set, omitting the isInvoking requirement

## Cost

Workers $0.24 (octocode $0.14, rg-gh $0.10) · judge $0.47 · total $0.71.

## Isolation

Every run passed its profile isolation check (offered/called tools, MCP servers, no skills, no benchmark-file access).
