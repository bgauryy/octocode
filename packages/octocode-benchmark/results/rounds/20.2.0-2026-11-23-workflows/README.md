# v19 vs v20 vs `gh`: 10 workflow questions

Ten questions that each take several tools in sequence ([question set](../../../questions/workflows/README.md)). The
upgrade guide that summarizes them is [19.1.0 to 20.2.0](../../../../../docs/V19_V20.md).

## Setup

| | |
|---|---|
| Versions | v19 (19.1.0), v20 (20.2.0), and read-only `gh` piped into text filters that read only from the pipe |
| Agent | Claude Sonnet 5.5, a fresh session per question and version, with only that version's tools, the question and one sentence asking it to cite its evidence |
| Questions | W1–W6 on public GitHub repositories and packages (all three); W7–W10 on pinned clones (v19 and v20) |
| Judge | Claude Opus 5.5, blind, against the source and an answer key. Every gap of 2 or more points was graded again in reverse order; the score is the mean of both gradings |
| Tokens | the provider's usage per run: new input (uncached plus cache writes), cached re-reads and output; cost prices a session's first write of the tool descriptions as a re-read |

## Results

| W1–W6, GitHub | v19 | v20 | `gh` |
|---|---:|---:|---:|
| Answer quality, mean (0–10) | 7.58 | 10.00 | 9.33 |
| Failed tool calls | 0 of 26 | 1 of 23 | 2 of 22 |
| Time per question, median | 19.8 s | 18.0 s | 18.2 s |
| Tokens per question, median: input (new + cached) / output | 106k (6.8k + 99.3k) / 2.0k | 106k (10.1k + 97.4k) / 1.7k | 26.5k (3.4k + 22.3k) / 1.6k |
| Model cost | $0.45 | $0.46 | $0.24 |

| W7–W10, code on disk | v19 | v20 |
|---|---:|---:|
| Answer quality, mean (0–10) | 9.62 | 8.88 |
| Failed tool calls | 7 of 28 | 4 of 31 |
| Time per question, median | 37.8 s | 30.6 s |
| Tokens per question, median: input (new + cached) / output | 243k (25.4k + 215k) / 3.9k | 245k (15.3k + 227k) / 3.4k |
| Model cost | $0.64 | $0.45 |

- v20 beat v19 on W1 and W6, the package advisory questions v19 has no tool for, and beat `gh` on W1, where `gh`'s
  answer gave its own list of 34 advisories as 33.
- v20 lost W7 to v19: asked how many direct calls test files make to a method, it estimated about 205 of 208, where
  v19 subtracted the calls made through an interface. Telling a direct call from one through an interface is not
  yet something `lspSearch` states.
- `gh` read the least on every question: its agent filters each reply with `jq` before reading it.

## Per question

| Q | Question | Version | Correct | Calls (failed) | Time | Input: new + cached | Output | Cost |
|---|---|---|---:|---:|---:|---:|---:|---:|
| W1 | Axios advisories and the SSRF fix | v19 | 4 | 6 (0) | 37 s | 11.5k + 178k | 2.9k | $0.093 |
| W1 | Axios advisories and the SSRF fix | v20 | 10 | 4 (0) | 26 s | 17.8k + 164k | 2.6k | $0.114 |
| W1 | Axios advisories and the SSRF fix | `gh` | 8 | 3 (0) | 24 s | 10.0k + 26.8k | 3.1k | $0.073 |
| W2 | Click 8.1.7 to 8.1.8 | v19 | 8 | 8 (0) | 42 s | 22.1k + 353k | 3.8k | $0.162 |
| W2 | Click 8.1.7 to 8.1.8 | v20 | 10 | 2 (0) | 18 s | 11.3k + 81.3k | 1.7k | $0.070 |
| W2 | Click 8.1.7 to 8.1.8 | `gh` | 9 | 4 (0) | 20 s | 7.8k + 23.0k | 1.9k | $0.052 |
| W3 | Vite's server.warmup option | v19 | 9 | 3 (0) | 18 s | 5.3k + 97.3k | 1.5k | $0.046 |
| W3 | Vite's server.warmup option | v20 | 10 | 6 (0) | 20 s | 8.1k + 111k | 1.9k | $0.063 |
| W3 | Vite's server.warmup option | `gh` | 10 | 3 (0) | 17 s | 2.7k + 20.8k | 1.1k | $0.024 |
| W4 | Pallets repositories and their Python floor | v19 | 10 | 3 (0) | 16 s | 6.3k + 97.8k | 1.2k | $0.047 |
| W4 | Pallets repositories and their Python floor | v20 | 10 | 2 (0) | 18 s | 5.4k + 81.5k | 1.0k | $0.039 |
| W4 | Pallets repositories and their Python floor | `gh` | 10 | 4 (2) | 20 s | 2.3k + 26.0k | 1.6k | $0.028 |
| W5 | Go http.Server.Shutdown | v19 | 9 | 2 (0) | 18 s | 5.0k + 64.1k | 2.1k | $0.048 |
| W5 | Go http.Server.Shutdown | v20 | 10 | 4 (0) | 17 s | 8.8k + 83.4k | 1.8k | $0.062 |
| W5 | Go http.Server.Shutdown | `gh` | 9 | 4 (0) | 14 s | 3.7k + 20.8k | 1.4k | $0.031 |
| W6 | Requests advisories and the .netrc fix | v19 | 5.5 | 4 (0) | 21 s | 7.3k + 101k | 1.8k | $0.058 |
| W6 | Requests advisories and the .netrc fix | v20 | 10 | 5 (1) | 18 s | 20.7k + 135k | 1.7k | $0.113 |
| W6 | Requests advisories and the .netrc fix | `gh` | 10 | 4 (0) | 16 s | 3.2k + 21.5k | 1.5k | $0.030 |
| W7 | Callers of Head.Appender | v19 | 9.5 | 7 (1) | 61 s | 24.1k + 219k | 4.3k | $0.161 |
| W7 | Callers of Head.Appender | v20 | 5.5 | 8 (0) | 36 s | 24.6k + 270k | 3.6k | $0.162 |
| W8 | Django's largest modules | v19 | 10 | 5 (1) | 24 s | 11.1k + 210k | 2.6k | $0.091 |
| W8 | Django's largest modules | v20 | 10 | 3 (1) | 15 s | 3.9k + 84.0k | 1.7k | $0.041 |
| W9 | Unsafe code in Tokio's AsyncRead impls | v19 | 9 | 10 (4) | 43 s | 26.8k + 255k | 5.6k | $0.189 |
| W9 | Unsafe code in Tokio's AsyncRead impls | v20 | 10 | 6 (1) | 28 s | 9.6k + 185k | 3.3k | $0.089 |
| W10 | Excalidraw's delete action | v19 | 10 | 6 (1) | 33 s | 36.6k + 206k | 3.6k | $0.203 |
| W10 | Excalidraw's delete action | v20 | 10 | 14 (2) | 33 s | 21.0k + 328k | 3.8k | $0.154 |
