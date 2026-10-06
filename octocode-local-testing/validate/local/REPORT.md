# Octocode local tools vs expert shell: head-to-head

Date: 2026-09-30.

**Versions:** octocode 19.2.0 (native 20.0.0), ripgrep 14.1.1, ast-grep 0.45.0, rust-analyzer 1.96.1, clangd 21, BSD ctags. fd, tree, tokei, gopls and jdtls are not installed; the shell arm used find, `git ls-files` and `wc -l` instead.

**Files in this folder:**
- **Harness:** `lib.mjs`, per-area scripts, `tables.mjs`, `run-all.sh`.
- **Raw data:** `raw/*.json`. The full generated tables are in `raw/tables.md`.
- **Fixtures:** `fixtures/edge/proj` (edge cases) and `fixtures/rewrite/`.

## Method
**Repos:**
- tsx (excalidraw)
- typescript-go (3.2 MB CRLF checker.ts)
- tokio
- prometheus
- django
- guava
- redis
- nlohmann/json

**Arms:** one expert shell command and one expert octocode query per task. The shell arm used rg flags such as `-w -F -t -g -U -l -c -C -m -S`. Octocode used `pageSize`, `resultView` and `regex:"literal"`.

**Measures:**
- **chars:** everything the agent must read, including every continuation.
- **ms:** median of 3 runs (LSP 2 runs, cold on each CLI call). The machine was loaded by concurrent builds, so timings are noisy.

**Ground truth** was built independently:
- `git grep` and Python `re`;
- `git ls-files`;
- the ast-grep CLI;
- ast-grep kind rules;
- every `rg -w` reference hit, adjudicated by reading the code;
- Python ast, resolvers for TS, Go and C, and Java import checks;
- `diff -r`.

## Summary by tool
| Tool | Shell chars / calls | Octocode chars / calls | Correctness | Rating |
|---|---|---|---|---|
| localSearch vs rg (17 tasks) | 260,775 / 17 (53,202 excluding the hot file) | 232,963 / 25 (68,002 excluding the hot file) | Both exact on 17/17 | 7 |
| localFetch vs sed / rg -C / head / tail (25 tasks) | 48,581 / 25 | 78,575 / 28 | Both byte-exact on 22/22 reads. Octocode outlines cost 1.7–10.4× more | 7 |
| structureSearch vs git ls-files / find / wc (18 tasks) | 36,939 / 18 | 69,630 / 29 | Names and top-N exact. The tree view hides tracked `.env.*` and `.npmrc` (recall 0.969) | 6 |
| astSearch match vs ast-grep (7 tasks) | 95,658 / 9 | 246,355 / 17 | TS `new Error($MSG)`: 13 of 19 found. Go and C bare patterns are right on the first try, while the CLI misparses them | 6 (match about 5) |
| astSearch symbols vs rg / ctags (6 files) | 37,253 / 6 | 64,188 / 6 | Octocode exact on 6/6. Shell precision 0.81–0.98 on Java, TS and C | (symbols about 8) |
| lspSearch vs rg -w | | | Rust, TS and Python references exact (rg precision 0.26–0.62; rg missed an aliased call). C without compile_commands found 2 of 27. Go and Java: server missing, clean error | 8 when a server exists |
| astTopology vs rg import grep | | | Correct at the repo root; beats rg's false positives on Python. A subdirectory root returns a false-complete 0 (Go, Python). Misses Java same-package use | 5 |
| astRewrite vs ast-grep and sed (5 codemods) | ast-grep 126k | 608k | Octocode identical to ast-grep 5/5. sed was wrong on 4/5 (rewrote doc comments, missed multi-line calls). Octocode's preview is 4.9× the chars | 7 |

## Edge cases
| Scenario | Shell | Octocode | Verdict |
|---|---|---|---|
| 166-hit identifier in the 3.2 MB checker.ts | 16,530 chars | 2,741 chars on page 1, reports a total of 166 and gives a continuation | octocode |
| Read the whole 3.2 MB file | cat: 3,151,772 chars | 18,355 chars on page 1, plus the next page | octocode |
| Symbols in checker.ts | 2,446 function lines | 12 declarations | shell (D2) |
| 2 MB single-line minified JS | naive rg: 2,077,782 chars; expert `rg -o` pattern: 138 | 710 chars, clipped and flagged truncated | octocode vs naive, shell vs expert |
| Binary read | dumps bytes | `binaryFileUnsupported` | octocode |
| Latin-1 read | cat works | refused as binary | shell (D6) |
| Symlink or `..` escape | `rg -L` follows and leaks | skipped or refused | octocode |
| node_modules, target, dist (not gitignored) | rg lists all 8 | lists src only by default; `defaultExcludes:false` gives 8 | octocode (better default) |
| Zero results, invalid regex | silent exit 1; a parse error | `empty` plus repair hints; `invalidRegex` plus `next.repair` | octocode |
| Invalid AST pattern | ast-grep warns about an ERROR node | reports `complete` and empty | shell (D8) |
| Fake secrets (AWS, `ghp_`, DB URL, RSA key) | rg and ast-grep leak all 3 checked values | 0 of 5 leak, with a "not verbatim" warning | octocode |
| `.env` read | readable | refused | octocode |
| Stale rewrite apply | ast-grep has no guard | `snapshot_changed`, file untouched | octocode |
| Emoji columns | rg reports bytes | octocode reports UTF-16 offsets, per its contract | correct |

## Wins and losses
**Octocode wins:**
- **LSP semantics:** precision 1.0 against 0.26–0.62. Definitions take 1 call against 2–4.
- **Syntax-aware symbols.**
- **Codemods vs sed.**
- **Go and C bare patterns**, which the ast-grep CLI misparses.
- **Hostile inputs:** huge files, minified code, secrets, escapes, binary files, `.env` and stale rewrites.
- **Honest paging with totals.**

**Octocode loses:**
- **Latency:** about 200 ms or more per CLI call against 3–25 ms, and LSP calls take seconds.
- **Chars:** 1.2–2.5× on plain text and layout tasks. The causes are the JSON envelope, `moreLines`, `next.clasify` payloads and rewrite previews.
- **Extra calls:** tail reads, trees over 100 entries, match paging.
- **Silent recall loss:** D1–D4.

## Defects
Repros use `cd octocode-local-testing/repos` or `fixtures/edge/proj` with `OCTOCODE_ENABLE_LOCAL=true`. Topology and rewrite also need `OCTOCODE_BETA=1`.

- **D1. astSearch match is stricter than ast-grep and than astRewrite.**
  - Repro: `new Error($MSG)` on tsx `elbowArrow.ts`.
  - Octocode returns 0 matches. ast-grep finds matches at lines 364, 755 and 826: multi-line calls with a trailing comma.
  - astRewrite, given the same pattern, matches all 19.
  - Unterminated patterns such as `export const $N = $V` also return 0.
- **D2. Symbols omit nested TypeScript functions.**
  - Repro: `nested/n.ts` misses innerA, innerB and deep. Python and Rust nested functions are listed.
  - On checker.ts, octocode returns 12 declarations; the file has 2,446 function lines.
- **D3. Topology falsely reports complete zero results when the root is a subdirectory.**
  - Go: root `go/tsdb` gives 0 results for chunkenc, which has 33 dependents. `../go.mod` is ignored.
  - Python: root `python/django` with `utils/text.py` gives 0; the truth is 20.
- **D4. Topology misses Java same-package usage.**
  - `Multimaps.java` uses `Lists.transform` without an import and is not listed.
  - The result still says `results:"complete"`.
- **D5. Single-file astSearch match silently drops redacted matches.**
  - Repro: in `s2/k.ts`, searching the directory returns 3 rows; searching the file returns 2. The `GITHUB_TOKEN` row is missing.
  - Env-var redaction also erases the identifier.
- **D6. localFetch refuses Latin-1 text as binary.** There is also no encoding option on search.
- **D7. clangd without `compile_commands.json` gives single-file references with no hint.**
  - It found 2 of 27 references.
  - The only signal is `coverage.exhaustive:false`.
- **D8. Invalid patterns and binary skips are reported inconsistently.**
  - `foo(` returns empty with `complete:true`.
  - The "binaryFileSkipped" warning does not name the file.
  - A NUL byte before the needle produces no warning at all.
- **D9. structureSearch and localSearch exclude different files.**
  - The tree view hides tracked `.env.*` and `.npmrc`, yet lists a gitignored `Cargo.lock`.
  - localSearch never searches `Cargo.lock`.
- **D10. Output weight.**
  - `moreLines` costs 1,011 chars against 48 for rg `-m1`; on hedley.hpp the list is 5.7k.
  - The embedded `next.clasify` hint and the symbols outline print import lines.
  - An astRewrite preview costs about 1.44 KB per match.
- **D11. Call-count friction.**
  - The tree `pageSize` is capped at 100.
  - There is no tail offset, so a tail read takes 2 calls.
