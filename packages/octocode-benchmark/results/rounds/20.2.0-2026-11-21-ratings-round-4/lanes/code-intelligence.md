# Round 4 — Code intelligence lane (astSearch, lspSearch)

Build: `octocode 20.2.0+56f67471`, binary sha256 `e246ff42…cff6a405f` (packages/octocode-native/npm/darwin-arm64/octocode), HEAD e9309aa3, dirty tree (97 entries). Ground truth: rg 14, ast-grep 0.45.0, reading each site.

## Table

| tool | Desc | Schema | Workflow | Clarity | Time | Tokens | Paging | Smart | Short | Long | Stress | Hints | Overall | Correct |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| astSearch | 9 | 8 | 9 | 8 | 9 | 8 | 9 | 9 | 9 | 9 | 8 | 7 | **8** | 13/14 checks (93%) |
| lspSearch | 8 | 8 | 9 | 8 | 6 | 8 | 9 | 7 | 9 | 9 | 7 | 8 | **8** | 14/15 checks (93%) |

Calls/chars (outputs incl. page walks): astSearch ~40 calls, ~405K chars; lspSearch ~20 calls, ~143K chars.

- `in: localSearch, lspSearch(definition) → astSearch → out: astSearch syntaxTree, localFetch, lspSearch`
- `in: localSearch hit, astSearch, workspaceSymbol → lspSearch → out: localFetch, localSearch (text lead), lspSearch references/implementation`

Reasons for scores ≤7:
- astSearch Hints 7: an empty row in a file with parse recovery carries no warning (D1), while a non-empty row in the same file does.
- lspSearch Time 6: Rust `references` of `ErrorCode` (engine/src/error.rs:114) 106.5 s cold, 19.5 s warm (482 text-candidate files); other calls 0.1–7 s.
- lspSearch Smart 7: `workspaceSymbol` with `workspaceRoot` only picks a `.cjs` script outside the tsconfig project (D2).
- lspSearch Stress 7: same `ErrorCode` latency.

## Defects

| id | sev | tool | input | observed vs expected | ground truth | owner |
|---|---|---|---|---|---|---|
| D1 | high (gate: silent loss) | astSearch | `{"operation":"match","path":"packages/octocode-native/crates/runtime-base/src/security/content/tests/key_blocks.rs","rule":{"pattern":"$X.clone()"}}` | status `empty`, `filesScanned:1`, no warning, no partialReasons (only a `related` syntaxTree at :308). Same for `hide_key_line($X)` (0 vs 2) and `$X.unwrap_or_default()` (0 vs 4). Dir scope `…/content/tests`: key_blocks.rs absent, no warning. Non-empty rows in the same file do warn (`redact_private_key_blocks($X)` 5 vs 15, `kind:for_expression` 1 vs 15, both `parseRecovered`). Expected: parseRecovered warning on every row whose scanned files parsed with errors, empty or not; and ideally no parse damage | ast-grep raw file: clone 5 (308,337,339,340,341), unwrap_or_default 4, for 15; raw file has 0 ERROR nodes; octocode syntaxTree: `ERROR 136:1-489:2`. Cause: redaction at :139 replaces `\n{body}\n");` with `[REDACTED-PRIVATEKEYFRAGMENT]`, swallowing the closing quote (localFetch 136–144 shows it) — R3-K2 class, a different shape (unterminated block to EOF) | native (redaction-before-parse + empty-row disclosure) |
| D2 | med-low | lspSearch | `{"operation":"workspaceSymbol","workspaceRoot":"packages/octocode-config","symbolFuzzy":"OctocodeEnv"}` | empty, "searched only the project of scripts/check-core-contract-sync.cjs" (disclosed via partialReasons/hint/related). Expected: anchor a file in the configured tsconfig project (src/…); with `path:"packages/octocode-config/src/dotenv.ts"` it returns ≥9 symbols | tsconfig.json `include: [scripts/**/*, src/**/*, tests/**/*]`, no allowJs: the .cjs gets an inferred project; rg finds 9+ `*OctocodeEnv*` declarations in src/dotenv.ts | native |
| D3 | low | lspSearch | `references` `ErrorCode` engine/src/error.rs:114 | 106.5 s cold / 19.5 s warm, answer correct (60 rows, 7 files) | — | native (perf) |
| D4 | low | lspSearch | `definition` `hide_key_line` at line 20 of key_blocks.rs (declared at 501) | resolves silently to :501 with `resolvedSymbol.lineDeviation:481`, no warning. Expected: a warning when the anchor moves hundreds of lines | rg: only one occurrence in the file | native |
| D5 | low | lspSearch | `callers` on enum | reason text "declares a enum" (article) | — | core/native text |
| D6 | low | astSearch | `astSearch --help` | "Required fields" lists `match (with view) path, operation, rule, view` — a redundant row; `maxDepth` on syntaxTree refused (fine, message good) | — | core |

## Verified answers

astSearch
- `Box::new($X)` tool-lsp/src countMatches: 31 in 8 files = ast-grep (exact per file). cold 0.33 s / warm 0.18 s.
- TS containment `for_statement has await` agents-communication/src: 6 files = ast-grep.
- `$X.clone()` over crates countMatches: 7399 / 1824 files, 11 pages, every file once, totals stable. ast-grep 6747 / 1761. Diff = octocode matches inside macro bodies (verified lifecycle.rs: 6 = rg 6, ast-grep 3) → octocode more complete; only miss = key_blocks.rs (D1).
- `JSON.parse($X)` packages **/*.ts content walk: 3 pages, 278 = ast-grep 278, same (path,line) set, 0 dup.
- `console.log($$$A)` repo root countMatches: 481 / 143 files = ast-grep ts+tsx+js exactly, 0.86 s.
- `$X.expect($Y)` key_blocks.rs: 3 = ast-grep 3 (R3-K2 input fixed).
- symbols `isUuid` exact; syntaxTree line 84 of validation.ts correct.
- Error inputs: bad pattern, unknown kind (nearest kinds listed), missing path (related structureSearch) — all correctInput with good repair.

lspSearch
- Rust references `hide_key_line`: 18 rows / 7 files = rg -w line set exactly; cold 6.9 s, warm 0.09 s; forced paging (responseChars 1500) 3 pages, 18 unique rows once.
- `key_fragment_placeholder` view files: 33 lines / 22 files = rg exactly.
- `ErrorCode` (engine): 60 rows; extra vs rg = `Self` references in impl (real); failure.rs RpcCode alias uses (real); 21 failure.rs lines are contract_types::ErrorCode (another symbol), correctly left out and disclosed as unverified; error.rs:61 doc link text-only; differential.rs:164 string literal text-only.
- TS callers `parseEnv`: 2 in dotenv.ts + 13 test sites = rg call lines.
- implementation trait `ContentScan`: 35 = rg `impl … ContentScan for` 35 (lsp + native-graph-facts rows, partialReasons scanOnlyImplementations).
- hover, typeDefinition (`LoadOctocodeEnvResult` :86), callees, documentSymbols on .md (serverUnavailable + localFetch symbols lead), unknown symbol (notFound + workspaceSymbol lead): correct.
- Python diagnostic relay.py: 199 grouped rows over 4 pages, 0 dup; 12 errors (grouped sites) = definition's `errorCount:12`.

## Regression check (round 3)

| id | status | evidence |
|---|---|---|
| R3-K1 | fixed | isUuid 14/14, decodeUtf8 20/20, exact rg line sets, coverage complete |
| R3-K2 | partly fixed | the round-3 input now 3/3; same file still loses matches silently (D1) |
| R3-K3 | fixed | callers `ErrorCode` 0.06 s, category notCallable, related references + implementation |
| R3-K4 | fixed | definition `running` relay.py:120 → hosts.py:95 `def running` |
| R3-K5 | fixed | workspaceRoot inferred `skills/octocode-messages-proxy/scripts`; 7.0 s cold, 0.15 s warm (was 49 s / 15–22 s) |
| R3-K6 | fixed | TS `"node:fs"` = `'node:fs'` = 152 files (rg 153; the extra is a default import, correctly unmatched); Python `'start'` pattern matches `"start"` source |
| R3-K7 | fixed | count walk: no isPartial without reason, skippedGrammars/warnings on first page only; `inferredLanguage` and `skippedGrammars` in output reference |

## Good
- astSearch matches inside Rust macro bodies (beyond ast-grep) and equals ast-grep elsewhere, fast at repo scale, with exact paging.
- lspSearch references are exact on TS and Rust and separate other-symbol text hits from misses with a clear unverified list.
