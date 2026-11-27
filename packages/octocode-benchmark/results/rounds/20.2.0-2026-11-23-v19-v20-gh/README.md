# v19 vs v20 vs `gh`: 60 research questions

The upgrade guide with these results is [19.1.0 to 20.2.0](../../../../../docs/V19_V20.md). This page keeps the
per-question numbers.

## Setup

| | |
|---|---|
| Versions | v19 (19.1.0, without its graph tool), v20 (20.2.0, without `clasify`), and read-only `gh`, whose output could be piped into text filters that read only from the pipe |
| Agent | Claude Sonnet 5.5, a fresh session per question and version, with only that version's tools, the question and one sentence asking it to cite its evidence |
| Questions | all 30 [GitHub questions](../../../questions/github/README.md) (three versions) and all 30 [questions on code on disk](../../../questions/local-core/README.md) (v19 and v20), on pinned commits |
| Judge | Claude Opus 5.5, blind, checking every answer against the source: three answers at once on GitHub, two with the answer key on disk. Every gap of 2 or more points was graded again in reverse order; the score is then the mean of both gradings, and a win counts only when both gradings agree |
| Tokens | the provider's usage per run: new input (uncached plus cache writes), cached re-reads (billed at 1/40 of a cache write) and output |
| Load | six sessions at a time, sharing one GitHub token |

## Results

| GitHub questions | v19 | v20 | `gh` |
|---|---:|---:|---:|
| Answer quality, mean (0–10) | 8.95 | 9.45 | 9.55 |
| Answers scored 8 or more | 28 of 30 | 29 of 30 | 29 of 30 |
| Answers scored 10 | 15 of 30 | 19 of 30 | 23 of 30 |
| Against v19: won / tied / lost / unresolved | — | 3 / 25 / 1 / 1 | 5 / 23 / 1 / 1 |
| Failed calls | 10 of 110 | 9 of 101 | 20 of 136 |
| Calls per question | 3.7 | 3.4 | 4.5 |
| Characters per question, median | 15.8k | 11.6k | 8.5k |
| Time per question, median | 21.5 s | 23.3 s (19.0 s without quota waits) | 17.9 s |
| Input tokens per question, new + cached, median | 116.1k | 105.3k | 26.1k |
| ↳ new input tokens, median | 7.3k | 3.6k | 4.4k |
| ↳ cached re-reads, median | 107.9k | 104.3k | 21.8k |
| Output tokens per question, median | 1.8k | 1.8k | 1.7k |
| Cost, 30 questions | $2.07 | $1.42 | $1.23 |
| Cost with tool descriptions already cached | $2.07 | $1.42 | $1.16 |

| Code on disk | v19 | v20 |
|---|---:|---:|
| Answer quality, mean (0–10) | 9.68 | 9.73 |
| Answers scored 8 or more | 29 of 30 | 29 of 30 |
| Answers scored 10 | 26 of 30 | 27 of 30 |
| Against v19: won / tied / lost / unresolved | — | 2 / 27 / 1 / 0 |
| Failed calls | 57 of 152 | 8 of 80 |
| Calls per question | 5.1 | 2.7 |
| Characters per question, median | 12.5k | 5.6k |
| Time per question, median | 17.3 s | 11.1 s |
| Input tokens per question, new + cached, median | 150.7k | 73.3k |
| ↳ new input tokens, median | 6.7k | 2.1k |
| ↳ cached re-reads, median | 140.9k | 70.8k |
| Output tokens per question, median | 2.1k | 1.1k |
| Cost, 30 questions | $1.98 | $0.87 |
| Cost with tool descriptions already cached | $1.98 | $0.87 |

- GitHub: v20 beat v19 on Q10, Q14 and Q22 in both gradings and lost Q23, where it credited one function's lines to
  the wrapper that calls it; Q25 went to v20 in one grading only. On Q14 v19 placed `vite` under `dependencies` from a
  line that sits in `devDependencies`. `gh` beat v19 on Q10, Q12, Q14, Q22 and Q23 and lost Q27. v20 beat `gh` on Q27,
  where `gh` named only part of the exports, and lost Q12 and Q23.
- Code on disk: v20 beat v19 on Q8 and Q24 in both gradings and lost Q9, where it found two of the four functions that
  skip `asyncify`.
- Time on GitHub: v20 waited 412 s in 12 of its 30 runs for GitHub's code-search quota (10
  searches a minute per token), which the parallel sessions used up together. `gh` sends without waiting, and v19 waits
  only after GitHub refuses a search. The figure in brackets leaves those waits out.
- Failed calls count replies marked as errors or holding an error row: Octocode's error rows, a read-only wrapper's
  refusal, or `gh`'s own `(HTTP 4xx)` error. Of `gh`'s 20, 11 were shell forms the agent's sandbox refuses (shell
  variables, some chained commands, `sleep`), 4 were GitHub refusing Chromium file reads with HTTP 429, 3 were files
  not found (HTTP 404) and 2 were other errors.
- Cache: every v19 and v20 session found its tool descriptions already cached; six `gh` sessions wrote theirs (about
  3k tokens each). The last cost row prices those writes as cache reads.

## Per question

### GitHub

| Q | Question | Version | Correct | Calls (failed) | Characters | Time | New input | Cached re-read | Total input | Output | Cost |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Q1 | Next.js route-regex result | v19 | 10 | 3 (0) | 8,411 | 12 s | 3.4k | 98.7k | 102.1k | 0.9k | $0.033 |
| Q1 | Next.js route-regex result | v20 | 10 | 2 (0) | 2,722 | 10 s | 1.0k | 84.2k | 85.2k | 0.7k | $0.019 |
| Q1 | Next.js route-regex result | `gh` | 10 | 3 (0) | 4,532 | 13 s | 4.4k | 19.1k | 23.6k | 1.0k | $0.030 |
| Q2 | Repository discovery and bounded absence | v19 | 10 | 4 (1) | 16,862 | 19 s | 7.3k | 141.6k | 148.9k | 1.6k | $0.060 |
| Q2 | Repository discovery and bounded absence | v20 | 10 | 4 (0) | 6,560 | 15 s | 2.8k | 85.2k | 88.0k | 1.5k | $0.034 |
| Q2 | Repository discovery and bounded absence | `gh` | 10 | 10 (2) | 8,558 | 26 s | 6.9k | 39.6k | 46.5k | 2.6k | $0.057 |
| Q3 | Flask route history | v19 | 10 | 3 (0) | 10,635 | 15 s | 0.0k | 104.9k | 104.9k | 1.5k | $0.026 |
| Q3 | Flask route history | v20 | 10 | 4 (1) | 8,577 | 16 s | 3.3k | 117.6k | 120.9k | 1.5k | $0.040 |
| Q3 | Flask route history | `gh` | 10 | 3 (0) | 6,603 | 12 s | 5.1k | 14.7k | 19.8k | 1.1k | $0.033 |
| Q4 | Axios redirect implementation across repositories | v19 | 8 | 4 (0) | 14,691 | 21 s | 7.3k | 135.7k | 142.9k | 1.9k | $0.061 |
| Q4 | Axios redirect implementation across repositories | v20 | 8 | 6 (1) | 17,728 | 27 s | 5.7k | 218.3k | 224.0k | 2.6k | $0.070 |
| Q4 | Axios redirect implementation across repositories | `gh` | 9 | 3 (0) | 7,967 | 15 s | 5.2k | 14.6k | 19.8k | 1.5k | $0.037 |
| Q5 | Vue hydration diff review | v19 | 9 | 2 (0) | 26,937 | 22 s | 0.0k | 110.3k | 110.3k | 2.4k | $0.035 |
| Q5 | Vue hydration diff review | v20 | 9 | 2 (0) | 25,153 | 19 s | 7.4k | 87.5k | 94.8k | 2.0k | $0.058 |
| Q5 | Vue hydration diff review | `gh` | 10 | 5 (1) | 40,102 | 30 s | 16.9k | 34.1k | 51.0k | 3.2k | $0.103 |
| Q6 | Express router cross-repository trace | v19 | 9 | 7 (1) | 16,674 | 23 s | 8.6k | 209.8k | 218.3k | 2.0k | $0.075 |
| Q6 | Express router cross-repository trace | v20 | 9 | 11 (1) | 10,861 | 72 s | 5.7k | 208.0k | 213.7k | 2.3k | $0.067 |
| Q6 | Express router cross-repository trace | `gh` | 8 | 3 (1) | 3,885 | 16 s | 4.0k | 18.8k | 22.8k | 1.1k | $0.029 |
| Q7 | Zustand's Next.js integration contract | v19 | 9 | 2 (0) | 16,276 | 13 s | 8.7k | 64.1k | 72.8k | 1.2k | $0.054 |
| Q7 | Zustand's Next.js integration contract | v20 | 10 | 1 (0) | 11,934 | 11 s | 5.3k | 56.0k | 61.3k | 1.0k | $0.037 |
| Q7 | Zustand's Next.js integration contract | `gh` | 10 | 3 (2) | 6,773 | 14 s | 3.4k | 16.3k | 19.7k | 1.3k | $0.028 |
| Q8 | VS Code keybinding dispatch | v19 | 10 | 3 (0) | 10,533 | 22 s | 5.9k | 97.8k | 103.7k | 1.3k | $0.046 |
| Q8 | VS Code keybinding dispatch | v20 | 10 | 3 (0) | 3,600 | 51 s | 1.3k | 84.4k | 85.7k | 1.1k | $0.025 |
| Q8 | VS Code keybinding dispatch | `gh` | 10 | 3 (0) | 4,126 | 9 s | 2.3k | 9.2k | 11.5k | 1.0k | $0.020 |
| Q9 | Fastify lifecycle contract | v19 | 10 | 2 (0) | 8,392 | 11 s | 4.5k | 64.1k | 68.6k | 1.1k | $0.035 |
| Q9 | Fastify lifecycle contract | v20 | 10 | 2 (1) | 6,813 | 47 s | 2.5k | 84.7k | 87.2k | 1.0k | $0.028 |
| Q9 | Fastify lifecycle contract | `gh` | 10 | 2 (0) | 4,726 | 9 s | 2.6k | 9.2k | 11.8k | 0.7k | $0.018 |
| Q10 | Axios repository and Node entry chain | v19 | 5.5 | 5 (0) | 15,342 | 17 s | 8.3k | 138.5k | 146.9k | 1.7k | $0.064 |
| Q10 | Axios repository and Node entry chain | v20 | 10 | 5 (0) | 7,887 | 36 s | 3.5k | 115.8k | 119.3k | 1.6k | $0.041 |
| Q10 | Axios repository and Node entry chain | `gh` | 10 | 5 (1) | 8,088 | 20 s | 4.3k | 29.6k | 33.9k | 1.7k | $0.037 |
| Q11 | Esbuild JavaScript-to-Go service boundary | v19 | 9 | 9 (6) | 24,807 | 46 s | 13.2k | 335.5k | 348.7k | 4.4k | $0.131 |
| Q11 | Esbuild JavaScript-to-Go service boundary | v20 | 10 | 5 (1) | 12,029 | 33 s | 5.3k | 116.7k | 122.1k | 2.2k | $0.055 |
| Q11 | Esbuild JavaScript-to-Go service boundary | `gh` | 10 | 9 (2) | 9,812 | 25 s | 5.1k | 29.3k | 34.4k | 2.8k | $0.052 |
| Q12 | Stream and EventEmitter wiring | v19 | 8 | 3 (0) | 11,421 | 23 s | 5.6k | 97.8k | 103.4k | 2.1k | $0.053 |
| Q12 | Stream and EventEmitter wiring | v20 | 8 | 1 (0) | 7,941 | 13 s | 2.6k | 56.0k | 58.7k | 1.3k | $0.029 |
| Q12 | Stream and EventEmitter wiring | `gh` | 10 | 3 (0) | 7,181 | 18 s | 3.1k | 15.8k | 18.9k | 1.8k | $0.032 |
| Q13 | Redis security issue and fix PR | v19 | 10 | 2 (0) | 20,295 | 16 s | 9.0k | 101.1k | 110.0k | 1.5k | $0.061 |
| Q13 | Redis security issue and fix PR | v20 | 10 | 4 (0) | 17,108 | 19 s | 7.9k | 86.8k | 94.7k | 1.8k | $0.059 |
| Q13 | Redis security issue and fix PR | `gh` | 10 | 5 (0) | 13,559 | 20 s | 6.8k | 32.0k | 38.8k | 1.5k | $0.045 |
| Q14 | Vitest’s dependency on Vite | v19 | 1 | 2 (0) | 4,336 | 9 s | 3.2k | 64.1k | 67.3k | 0.7k | $0.026 |
| Q14 | Vitest’s dependency on Vite | v20 | 10 | 1 (0) | 2,904 | 11 s | 1.0k | 56.0k | 57.0k | 0.7k | $0.016 |
| Q14 | Vitest’s dependency on Vite | `gh` | 10 | 2 (0) | 2,534 | 10 s | 1.8k | 14.6k | 16.4k | 0.9k | $0.018 |
| Q15 | Hono JSX array component PR | v19 | 10 | 3 (2) | 29,833 | 17 s | 13.9k | 108.0k | 121.9k | 1.4k | $0.081 |
| Q15 | Hono JSX array component PR | v20 | 10 | 2 (0) | 16,140 | 17 s | 1.0k | 94.3k | 95.3k | 1.2k | $0.026 |
| Q15 | Hono JSX array component PR | `gh` | 10 | 3 (0) | 8,066 | 14 s | 3.9k | 23.4k | 27.3k | 1.3k | $0.031 |
| Q16 | ESLint parser dependency chain | v19 | 10 | 5 (0) | 10,759 | 18 s | 6.6k | 136.4k | 143.0k | 1.2k | $0.052 |
| Q16 | ESLint parser dependency chain | v20 | 10 | 3 (0) | 3,619 | 16 s | 1.4k | 113.3k | 114.8k | 1.0k | $0.027 |
| Q16 | ESLint parser dependency chain | `gh` | 10 | 6 (0) | 4,130 | 20 s | 2.9k | 33.5k | 36.4k | 1.4k | $0.029 |
| Q17 | Next.js fetch request memoization | v19 | 10 | 3 (0) | 19,923 | 27 s | 8.8k | 98.1k | 106.9k | 3.1k | $0.076 |
| Q17 | Next.js fetch request memoization | v20 | 9 | 2 (0) | 11,105 | 47 s | 3.7k | 84.8k | 88.5k | 1.6k | $0.039 |
| Q17 | Next.js fetch request memoization | `gh` | 10 | 4 (0) | 12,039 | 20 s | 4.9k | 22.9k | 27.8k | 2.1k | $0.043 |
| Q18 | Vite dependency-section membership | v19 | 10 | 1 (0) | 10,303 | 24 s | 4.2k | 65.6k | 69.8k | 2.9k | $0.052 |
| Q18 | Vite dependency-section membership | v20 | 10 | 1 (0) | 3,565 | 14 s | 0.0k | 56.9k | 56.9k | 1.1k | $0.016 |
| Q18 | Vite dependency-section membership | `gh` | 10 | 3 (0) | 3,666 | 11 s | 2.3k | 15.0k | 17.3k | 1.1k | $0.022 |
| Q19 | Node child-process async and sync paths | v19 | 9 | 6 (0) | 28,941 | 31 s | 13.7k | 178.3k | 192.0k | 3.0k | $0.103 |
| Q19 | Node child-process async and sync paths | v20 | 9 | 3 (0) | 15,707 | 44 s | 6.4k | 117.6k | 124.0k | 2.3k | $0.060 |
| Q19 | Node child-process async and sync paths | `gh` | 10 | 5 (0) | 11,325 | 19 s | 4.9k | 15.3k | 20.2k | 2.5k | $0.046 |
| Q20 | Actions toolkit exec output path | v19 | 10 | 5 (0) | 19,200 | 24 s | 9.0k | 172.1k | 181.1k | 2.6k | $0.079 |
| Q20 | Actions toolkit exec output path | v20 | 10 | 4 (1) | 11,219 | 37 s | 4.3k | 115.1k | 119.4k | 2.0k | $0.049 |
| Q20 | Actions toolkit exec output path | `gh` | 10 | 3 (0) | 9,497 | 18 s | 4.0k | 23.3k | 27.3k | 1.8k | $0.036 |
| Q21 | LangChain createAgent flow and graph | v19 | 9 | 8 (0) | 66,842 | 42 s | 29.5k | 347.7k | 377.2k | 4.6k | $0.199 |
| Q21 | LangChain createAgent flow and graph | v20 | 9 | 5 (0) | 37,950 | 96 s | 15.4k | 159.7k | 175.1k | 3.7k | $0.115 |
| Q21 | LangChain createAgent flow and graph | `gh` | 8 | 6 (0) | 22,422 | 30 s | 9.3k | 63.5k | 72.8k | 3.0k | $0.074 |
| Q22 | Axios release-range compare | v19 | 8 | 2 (0) | 9,513 | 14 s | 1.9k | 101.6k | 103.5k | 1.2k | $0.030 |
| Q22 | Axios release-range compare | v20 | 10 | 2 (0) | 13,192 | 16 s | 0.6k | 95.3k | 95.9k | 1.2k | $0.024 |
| Q22 | Axios release-range compare | `gh` | 10 | 2 (0) | 11,001 | 14 s | 6.1k | 18.8k | 24.9k | 1.2k | $0.038 |
| Q23 | Linux write() syscall to VFS dispatch flow | v19 | 8 | 3 (0) | 12,964 | 23 s | 7.0k | 98.1k | 105.1k | 2.4k | $0.061 |
| Q23 | Linux write() syscall to VFS dispatch flow | v20 | 5 | 3 (1) | 7,770 | 76 s | 3.0k | 115.6k | 118.5k | 1.9k | $0.043 |
| Q23 | Linux write() syscall to VFS dispatch flow | `gh` | 10 | 2 (0) | 8,414 | 16 s | 4.2k | 16.4k | 20.6k | 1.7k | $0.036 |
| Q24 | Axios buildFullPath blast radius | v19 | 10 | 3 (0) | 14,069 | 21 s | 4.2k | 136.1k | 140.3k | 1.6k | $0.046 |
| Q24 | Axios buildFullPath blast radius | v20 | 9 | 4 (1) | 12,244 | 122 s | 4.7k | 118.4k | 123.1k | 2.2k | $0.052 |
| Q24 | Axios buildFullPath blast radius | `gh` | 8.5 | 8 (2) | 8,738 | 24 s | 4.5k | 26.4k | 30.8k | 2.3k | $0.044 |
| Q25 | Axios PR selected-patch and review thread | v19 | 8 | 2 (0) | 15,074 | 19 s | 5.6k | 100.5k | 106.1k | 1.9k | $0.051 |
| Q25 | Axios PR selected-patch and review thread | v20 | 9.5 | 4 (0) | 17,142 | 23 s | 7.3k | 115.9k | 123.2k | 2.1k | $0.062 |
| Q25 | Axios PR selected-patch and review thread | `gh` | 8 | 4 (0) | 10,617 | 22 s | 4.9k | 23.4k | 28.3k | 2.1k | $0.042 |
| Q26 | Zustand documentation surface | v19 | 10 | 4 (0) | 23,684 | 17 s | 10.9k | 131.5k | 142.4k | 1.4k | $0.071 |
| Q26 | Zustand documentation surface | v20 | 10 | 4 (0) | 8,239 | 18 s | 2.9k | 143.7k | 146.6k | 1.7k | $0.043 |
| Q26 | Zustand documentation surface | `gh` | 10 | 4 (0) | 2,946 | 13 s | 2.1k | 20.7k | 22.8k | 1.1k | $0.022 |
| Q27 | React Query public API surface | v19 | 10 | 3 (0) | 14,592 | 24 s | 6.2k | 101.3k | 107.4k | 1.8k | $0.053 |
| Q27 | React Query public API surface | v20 | 10 | 3 (0) | 12,994 | 18 s | 3.0k | 89.1k | 92.1k | 2.0k | $0.041 |
| Q27 | React Query public API surface | `gh` | 7 | 3 (0) | 9,344 | 17 s | 4.2k | 16.4k | 20.5k | 1.9k | $0.037 |
| Q28 | Hermes agent memory management | v19 | 9 | 4 (0) | 63,553 | 32 s | 27.8k | 152.5k | 180.3k | 3.1k | $0.157 |
| Q28 | Hermes agent memory management | v20 | 9 | 4 (0) | 38,571 | 93 s | 17.3k | 166.4k | 183.7k | 2.7k | $0.113 |
| Q28 | Hermes agent memory management | `gh` | 8 | 7 (1) | 13,656 | 23 s | 6.1k | 31.4k | 37.5k | 2.4k | $0.051 |
| Q29 | MCP HTTP transport authorization flow | v19 | 9 | 4 (0) | 31,729 | 24 s | 13.3k | 137.3k | 150.6k | 2.9k | $0.096 |
| Q29 | MCP HTTP transport authorization flow | v20 | 10 | 4 (1) | 20,345 | 24 s | 7.2k | 113.5k | 120.6k | 2.9k | $0.069 |
| Q29 | MCP HTTP transport authorization flow | `gh` | 10 | 8 (3) | 20,348 | 29 s | 8.6k | 53.4k | 62.0k | 3.4k | $0.074 |
| Q30 | Chromium inter-process communication | v19 | 10 | 3 (0) | 36,243 | 37 s | 15.1k | 107.8k | 123.0k | 3.2k | $0.103 |
| Q30 | Chromium inter-process communication | v20 | 10 | 2 (0) | 26,237 | 60 s | 8.9k | 56.1k | 65.0k | 2.7k | $0.068 |
| Q30 | Chromium inter-process communication | `gh` | 10 | 9 (5) | 18,588 | 36 s | 7.6k | 52.8k | 60.4k | 2.7k | $0.063 |

### Code on disk

| Q | Question | Version | Correct | Calls (failed) | Characters | Time | New input | Cached re-read | Total input | Output | Cost |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Q1 | Tokio's free spawn_blocking function | v19 | 10 | 4 (2) | 5,791 | 14 s | 4.0k | 132.6k | 136.7k | 1.5k | $0.045 |
| Q1 | Tokio's free spawn_blocking function | v20 | 10 | 3 (0) | 5,849 | 11 s | 2.6k | 86.3k | 88.9k | 1.0k | $0.029 |
| Q2 | Lodash chunk body | v19 | 9 | 9 (4) | 12,524 | 21 s | 7.6k | 243.5k | 251.1k | 2.2k | $0.077 |
| Q2 | Lodash chunk body | v20 | 10 | 3 (1) | 5,381 | 10 s | 2.1k | 56.0k | 58.2k | 1.2k | $0.026 |
| Q3 | Prometheus chunk encoding directory | v19 | 10 | 4 (1) | 6,066 | 16 s | 4.1k | 133.0k | 137.1k | 1.5k | $0.045 |
| Q3 | Prometheus chunk encoding directory | v20 | 9 | 2 (0) | 2,825 | 8 s | 0.0k | 57.0k | 57.0k | 0.9k | $0.015 |
| Q4 | Redis listpack encoding constant | v19 | 10 | 3 (0) | 4,887 | 12 s | 3.6k | 97.8k | 101.4k | 1.0k | $0.034 |
| Q4 | Redis listpack encoding constant | v20 | 10 | 1 (0) | 1,641 | 8 s | 0.0k | 56.6k | 56.6k | 0.4k | $0.010 |
| Q5 | Cats Functor type class | v19 | 10 | 6 (4) | 8,072 | 17 s | 5.4k | 236.4k | 241.8k | 1.9k | $0.064 |
| Q5 | Cats Functor type class | v20 | 10 | 4 (1) | 3,616 | 10 s | 1.6k | 113.6k | 115.2k | 1.0k | $0.028 |
| Q6 | Django get_random_string | v19 | 10 | 5 (2) | 10,924 | 14 s | 6.3k | 172.2k | 178.5k | 1.4k | $0.057 |
| Q6 | Django get_random_string | v20 | 10 | 3 (0) | 6,044 | 9 s | 2.6k | 84.9k | 87.5k | 0.8k | $0.027 |
| Q7 | Excalidraw isTextBindableContainer call sites | v19 | 10 | 8 (3) | 32,117 | 28 s | 14.8k | 269.7k | 284.6k | 3.7k | $0.123 |
| Q7 | Excalidraw isTextBindableContainer call sites | v20 | 10 | 7 (2) | 11,184 | 21 s | 5.2k | 116.3k | 121.5k | 2.5k | $0.057 |
| Q8 | Prometheus NewRangeQuery callers | v19 | 5 | 3 (1) | 7,999 | 13 s | 3.4k | 100.2k | 103.6k | 1.5k | $0.039 |
| Q8 | Prometheus NewRangeQuery callers | v20 | 10 | 1 (0) | 7,348 | 10 s | 2.7k | 56.0k | 58.8k | 1.0k | $0.027 |
| Q9 | Tokio fs functions that skip asyncify | v19 | 8.5 | 7 (3) | 32,097 | 26 s | 14.6k | 188.0k | 202.6k | 3.7k | $0.114 |
| Q9 | Tokio fs functions that skip asyncify | v20 | 4 | 6 (1) | 17,362 | 22 s | 7.9k | 182.6k | 190.6k | 2.4k | $0.074 |
| Q10 | Django SuspiciousOperation subclasses | v19 | 10 | 6 (2) | 16,300 | 17 s | 8.9k | 138.6k | 147.5k | 2.2k | $0.072 |
| Q10 | Django SuspiciousOperation subclasses | v20 | 10 | 2 (0) | 9,318 | 12 s | 3.6k | 56.0k | 59.6k | 1.3k | $0.033 |
| Q11 | Django query.py outline | v19 | 10 | 6 (1) | 20,030 | 17 s | 8.6k | 142.9k | 151.5k | 2.2k | $0.070 |
| Q11 | Django query.py outline | v20 | 10 | 3 (0) | 6,342 | 10 s | 2.4k | 56.0k | 58.4k | 1.4k | $0.029 |
| Q12 | Excalidraw package dependency sections | v19 | 10 | 5 (3) | 12,582 | 26 s | 7.4k | 176.4k | 183.9k | 3.2k | $0.079 |
| Q12 | Excalidraw package dependency sections | v20 | 10 | 2 (0) | 3,265 | 8 s | 1.3k | 56.1k | 57.4k | 0.8k | $0.019 |
| Q13 | Tokio backtrace dependency | v19 | 10 | 5 (2) | 9,284 | 15 s | 5.8k | 170.8k | 176.6k | 1.4k | $0.055 |
| Q13 | Tokio backtrace dependency | v20 | 10 | 3 (0) | 4,296 | 10 s | 2.0k | 85.3k | 87.3k | 1.0k | $0.026 |
| Q14 | Guava ForwardingCollection subclasses | v19 | 10 | 5 (2) | 10,001 | 22 s | 4.5k | 174.6k | 179.1k | 2.0k | $0.055 |
| Q14 | Guava ForwardingCollection subclasses | v20 | 10 | 2 (1) | 5,125 | 12 s | 1.8k | 56.0k | 57.8k | 1.3k | $0.026 |
| Q15 | Redis setGenericCommand callers | v19 | 10 | 3 (2) | 10,516 | 14 s | 6.0k | 136.5k | 142.4k | 1.5k | $0.052 |
| Q15 | Redis setGenericCommand callers | v20 | 10 | 1 (0) | 3,671 | 10 s | 1.1k | 56.0k | 57.1k | 0.8k | $0.018 |
| Q16 | Newtonsoft license headers behind a BOM | v19 | 10 | 6 (4) | 12,432 | 24 s | 7.3k | 207.6k | 214.8k | 2.4k | $0.074 |
| Q16 | Newtonsoft license headers behind a BOM | v20 | 10 | 3 (0) | 5,845 | 14 s | 2.6k | 86.4k | 89.0k | 1.4k | $0.033 |
| Q17 | Checker declarations in an 8 MB fixture | v19 | 10 | 3 (2) | 8,790 | 14 s | 3.9k | 99.9k | 103.8k | 1.6k | $0.041 |
| Q17 | Checker declarations in an 8 MB fixture | v20 | 10 | 2 (0) | 2,442 | 11 s | 1.1k | 56.1k | 57.2k | 0.7k | $0.017 |
| Q18 | LangChain v1 langgraph pin and lock | v19 | 10 | 3 (1) | 8,818 | 11 s | 5.5k | 99.5k | 105.1k | 1.4k | $0.046 |
| Q18 | LangChain v1 langgraph pin and lock | v20 | 10 | 2 (0) | 3,227 | 11 s | 1.2k | 56.2k | 57.4k | 0.9k | $0.020 |
| Q19 | TypeScript assignability diagnostic codes | v19 | 10 | 3 (2) | 7,359 | 14 s | 4.6k | 98.6k | 103.2k | 1.7k | $0.045 |
| Q19 | TypeScript assignability diagnostic codes | v20 | 10 | 2 (0) | 5,122 | 11 s | 1.9k | 56.2k | 58.1k | 1.0k | $0.023 |
| Q20 | Histogram in Rust versus Go | v19 | 10 | 6 (1) | 13,598 | 15 s | 6.4k | 174.4k | 180.8k | 1.8k | $0.061 |
| Q20 | Histogram in Rust versus Go | v20 | 10 | 2 (1) | 5,244 | 10 s | 2.0k | 56.2k | 58.2k | 1.0k | $0.024 |
| Q21 | Django non-ASCII static file | v19 | 10 | 8 (2) | 25,005 | 19 s | 13.6k | 227.9k | 241.5k | 2.2k | $0.099 |
| Q21 | Django non-ASCII static file | v20 | 9 | 3 (0) | 7,021 | 14 s | 3.1k | 84.6k | 87.7k | 1.1k | $0.032 |
| Q22 | pkg/errors in Prometheus | v19 | 10 | 5 (1) | 20,696 | 16 s | 10.0k | 105.5k | 115.5k | 2.3k | $0.073 |
| Q22 | pkg/errors in Prometheus | v20 | 10 | 2 (0) | 5,646 | 12 s | 2.0k | 56.1k | 58.1k | 1.2k | $0.026 |
| Q23 | Prometheus target relabeling function | v19 | 10 | 7 (4) | 11,199 | 55 s | 6.6k | 243.1k | 249.7k | 2.5k | $0.076 |
| Q23 | Prometheus target relabeling function | v20 | 10 | 3 (0) | 4,952 | 11 s | 1.9k | 85.1k | 87.0k | 1.1k | $0.027 |
| Q24 | TypeScript compiler pins across three repos | v19 | 8 | 4 (2) | 16,342 | 18 s | 9.0k | 137.2k | 146.2k | 2.4k | $0.073 |
| Q24 | TypeScript compiler pins across three repos | v20 | 10 | 1 (0) | 4,771 | 9 s | 1.6k | 56.2k | 57.9k | 1.0k | $0.022 |
| Q25 | Kubelet eviction decisions | v19 | 10 | 6 (2) | 18,095 | 17 s | 9.8k | 179.6k | 189.4k | 2.2k | $0.079 |
| Q25 | Kubelet eviction decisions | v20 | 10 | 3 (0) | 10,174 | 14 s | 4.2k | 115.2k | 119.4k | 1.5k | $0.043 |
| Q26 | Sorted-set add option conflicts | v19 | 10 | 4 (0) | 12,082 | 28 s | 6.8k | 135.6k | 142.3k | 1.5k | $0.055 |
| Q26 | Sorted-set add option conflicts | v20 | 10 | 2 (0) | 5,563 | 12 s | 0.0k | 87.0k | 87.0k | 0.9k | $0.018 |
| Q27 | Tokio work-stealing search | v19 | 10 | 4 (1) | 23,856 | 24 s | 10.9k | 139.0k | 149.9k | 2.6k | $0.084 |
| Q27 | Tokio work-stealing search | v20 | 10 | 4 (0) | 11,493 | 19 s | 4.3k | 115.3k | 119.6k | 1.9k | $0.047 |
| Q28 | Django CSRF checks on HTTPS | v19 | 10 | 6 (1) | 18,699 | 21 s | 7.6k | 177.0k | 184.6k | 2.2k | $0.070 |
| Q28 | Django CSRF checks on HTTPS | v20 | 10 | 3 (1) | 8,062 | 13 s | 2.9k | 114.2k | 117.1k | 1.3k | $0.036 |
| Q29 | Guava cache weight eviction | v19 | 10 | 4 (0) | 15,473 | 24 s | 7.7k | 99.9k | 107.6k | 2.6k | $0.067 |
| Q29 | Guava cache weight eviction | v20 | 10 | 2 (0) | 8,467 | 13 s | 2.9k | 85.7k | 88.5k | 1.4k | $0.034 |
| Q30 | JSON lexer surrogate pairs | v19 | 10 | 4 (2) | 12,694 | 15 s | 6.2k | 136.4k | 142.5k | 1.6k | $0.054 |
| Q30 | JSON lexer surrogate pairs | v20 | 10 | 3 (0) | 7,358 | 13 s | 2.4k | 85.6k | 88.1k | 1.2k | $0.030 |
