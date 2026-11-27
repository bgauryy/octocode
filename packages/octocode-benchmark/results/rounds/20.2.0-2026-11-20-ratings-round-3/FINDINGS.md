# Findings: rating round 3, 2026-11-20

Every new finding from [rating round 3](SCORECARD.md) on `octocode 20.2.0+4476eb38`, as the raters reported them.
"Confirmed" means the coordinator re-ran it on the pinned binary. This copy does not change.

Severity: **high** wrong evidence, lost evidence or money spent without consent; **med** wrong lead, slow, or
costly; **low** wording, size or polish. Owner: **core** (agent text and schemas), **native** (Rust behavior).

## Code intelligence (astSearch 8, lspSearch 7)

| # | Tool | Finding | Evidence | Sev | Owner |
|---|---|---|---|---|---|
| R3-K1 | lspSearch | TypeScript references silently drop uses reached through a dynamic `import('../dist/…')`; the round-4 fix covers static imports of the built output only | `references` of `isUuid` (`agents-communication/src/runtime/validation.ts:84`): 7 in 4 files vs 14 (missing `tests/native-transports.test.ts:28,29`, `tests/runtime-edge-cases.test.ts:272–276`); `decodeUtf8` (`json-text.ts:14`) 18 vs 20. Confirmed | high | native |
| R3-K2 | astSearch | Key-block redaction runs before parsing and can swallow a closing quote, hiding a real match with no warning | `$X.expect($Y)` on `runtime-base/src/security/content/tests/key_blocks.rs`: 2 vs 3 (`ast-grep` on the raw file: 3). Confirmed | med (gate: silent) | native |
| R3-K3 | lspSearch | `callers` on an enum or trait takes 27 s and returns empty with a wrong diagnosis ("its project may not cover them") | `callers` `ErrorCode` at `engine/src/error.rs:114`; trait `ContentScan` (`scan.rs:12`) the same. Expected: say it isn't callable and offer `references`/`implementation` | med | native + core |
| R3-K4 | lspSearch (Python) | The anchor picks an occurrence inside a string literal; the right one only appears in `alternatives` | `definition` at `skills/octocode-messages-proxy/scripts/relay.py:120` (and `:118`) | med-low | native |
| R3-K5 | lspSearch (Python) | With no project marker, `workspaceRoot` is inferred as `.`, scanning the monorepo (49 s cold, 15–22 s warm) with no hint to narrow | same file; `workspaceRoot:"skills/octocode-messages-proxy"` takes 2.7 s | low-med | native |
| R3-K6 | astSearch | String quote style silently decides a match | TS `import { $$$A } from "node:fs"` empty vs `'node:fs'` 25 files; Python `add_parser('start')` vs `"start"` | low | native |
| R3-K7 | astSearch | Leftover page shape: `isPartial:true` beside `hasMore` with no reason on 18 of 20 countMatches pages; one near-empty page at a loaded-window edge; `skippedGrammars` repeats in the content walk; `inferredLanguage` not in the output reference | countMatches walk over `crates` | low | native + core |

## Graph (graphBuild 8, graphQuery 7)

| # | Tool | Finding | Evidence | Sev | Owner |
|---|---|---|---|---|---|
| R3-G1 | graphQuery | `impact` at `depth:1` leaves out direct callers that use path calls; the depth-1 rows and counts change with the depth asked for | engine scope, `impact` `src/num.rs#saturating_u32` `kind:"symbol"`: `byDepth.1` 24 at depth 1, 42 at depth 2; `callers view:files` and rg: 41 files. Confirmed | high | native |
| R3-G2 | graphQuery | `impact since:"HEAD"` seeds every file of a package from a one-line `package.json` scripts edit; seeded rows look like edited files | 3,546 files seeded as `via:"changed"`, 112 really changed; 3,571 rows over 72 pages (588 KB) | med | native |
| R3-G3 | graphBuild | `#[path]` on an inline module never links the nested module, which is then reported as unreachable at tier 90 | fixture `#[path="extra"] mod inl { pub mod deep; }`; same shape at `engine/build.rs:40-42` | low-med | native |
| R3-G4 | graphQuery | `impact` on a directory returns `notFound`, though `deps`/`dependents` accept directories | `impact target:"ts/src/cycle"` | low | native + core |
| R3-G5 | graphQuery | `mentionedIn` names a line that doesn't contain the path | `issues minTier:60`: `py/main.py` → `ts/package.json:1` | low | native |
| R3-G6 | graphQuery | 332 of 638 `testsToRun` rows have no `functions`, and the doc doesn't say what that means | `next.testsToRun` on `engine/src/num.rs` | low | core |

## Local (structureSearch 9, localSearch 8.5, localFetch 8)

| # | Tool | Finding | Evidence | Sev | Owner |
|---|---|---|---|---|---|
| R3-L1 | localFetch | `matchString` matches redaction placeholders, so counts differ from the source (round-4 L1 covered localSearch only) | `{"path":"skills/octocode-scraping/scripts/tests/har-ingest.test.mjs","matchString":["AWSACCESSKEYID"]}`: 2 vs rg 0; `"REDACTED"` 6 vs 0; `"TOKEN"` on `security_matrix.rs` 20 vs 19. Confirmed | med | native |
| R3-L2 | localSearch, localFetch | Hits inside masked spans are dropped from rows that have other hits without any note; meanwhile every empty row says "masked secrets are not searched" even when nothing was masked | `ghp_` on `runtime/src/runtime/engine/security_matrix.rs`: 1 vs rg 2 (line 17 holds a token); `ghp_` over `runtime/src/runtime/engine`: 15 vs 18 | med | native |
| R3-L3 | localFetch | `view:"symbols"` leaves out the body of an item-level macro call with no disclosure | `tool-local/src/structure_search/tree.rs`: lines 20–73 (`listing_fields!(…)`, 6 methods) missing | low | native |
| R3-L4 | localSearch | An empty result under `testCode:"exclude"` doesn't offer `testCode:"include"` | `runtime/src`, `fn \w*hint\w*[<(]`: 90 test files excluded, all 14 rg hits in them | low | native |
| R3-L5 | local tools | Clarity gaps: `matchedLineCount` omitted when equal to `matchCount` and match-only `value` omitted (undocumented); `testCodeExcluded` reads backwards with `testCode:"only"`; a sparse clone's `include:["tests/**"]` gets the generic glob hint; the `P1D` refusal example shows no `modifiedAt` | per rater | low | core + native |

## clasify (8)

| # | Finding | Evidence | Sev | Owner |
|---|---|---|---|---|
| R3-C1 | A 170 KB source with `maxRequests:1` spends a request, then stops: the hint says to raise `maxRequests` to `requestsCeiling`, which equals the cap; no `next.confirm`, no section count | source localFetch `packages/octocode-config/contract/contract-fixtures.json`; message from `tools-shared/src/judgment/cap.rs` `spent()` on the long-window retry | med | native |
| R3-C2 | A GitHub-file citation leaves out the deciding line | ripgrep `Cargo.toml`, edition question: answer `e2024` right, `lines` 12–28 (holds `edition.workspace = true`); the value `edition = "2024"` is at line 52 | med | native |
| R3-C3 | `responseChars` is ignored on clasify answer rows | `responseChars:500` on a 12-unit source: 769-char page, no pagination, no `oversized` | low | native |
| R3-C4 | Error examples for a nested `source.query` field or an `http://` URL show a localFetch source instead of the caller's form; unknown nested fields get no name suggestion | `pattern`, `contextLine` | low | native |
| R3-C5 | Quiet sections aren't shown on page 1 (`next.quiet` only on the last batch); a 3-resource glob page has no top-level `next.read` for the resource with a yes | sections walk; resources glob | low | native |
| R3-C6 | Price text says "at most 19 with every re-ask" while `requestsCeiling` is 22; `inputTokensAtMost` stated on one refusal and missing on two later ones | per rater | low | native |

## GitHub read (ghSearchRepo 9, ghSearchCode 8, ghStructure 8, ghGetFileContent 8.5)

| # | Tool | Finding | Evidence | Sev | Owner |
|---|---|---|---|---|---|
| R3-GR1 | ghSearchCode | A `ref` search misses files that match only at that ref; the warning implies they would appear, and there is no lead | `{"owner":"pallets","repo":"flask","ref":"2.0.0","keywords":["before_first_request"]}`: only `CHANGES.rst` (also `src/flask/app.py:1200`, `blueprints.py:481` at 2.0.0); tokio `tokio-1.0.0` `basic_scheduler` misses `tokio/src/runtime/mod.rs` | med | core + native |
| R3-GR2 | ghGetFileContent | `block:true` on Ruby/PHP/Kotlin reads from the hit to the end of the file instead of the block | sinatra `def route(` 7 lines vs a 398-line lead; laravel `slug(` 24 vs 690; rails `def match(` 2,000 lines | low-med | native |
| R3-GR3 | GitHub tools | A version tag spelled with or without `v` gets no repair; the leads read the default branch instead | ripgrep `ref:"v0.10.0"` (tag `0.10.0`), fd `ref:"10.2.0"` (tag `v10.2.0`) | low-med | native |
| R3-GR4 | GitHub tools | `owner`/`repo` given as a URL or `owner/repo` isn't repaired, and the error examples name repos that don't exist | `owner:"github.com/sharkdp"` → example `bgauryy/bat` (404) | low | native |
| R3-GR5 | ghStructure | An empty refs glob isn't marked empty and gets no lead | flask `9.9.*`, tokio `v99.*` | low | native |
| R3-GR6 | output reference | `returnedLines` and `defaultBranch` missing from `--view output`; a byte-paged long-line read switches unit on its last page; "whitespace-only-no-grammar" jargon | per rater | low | core + native |

## GitHub history (ghSearchHistory 9, ghGetHistoryItem 8, ghCloneRepo 9)

| # | Tool | Finding | Evidence | Sev | Owner |
|---|---|---|---|---|---|
| R3-H1 | ghGetHistoryItem | Patches GitHub withholds get a read only for the first file, and `terminalLimit` marks the others unreachable though each can be read at head or base | ripgrep#1017 `sections:["patches"]`: 12 such files, one `readAtCommit`; TypeScript#51387 with `include`: 18 files, one read | med | native |
| R3-H2 | ghSearchHistory, ghGetHistoryItem | "Who merged" can't be answered: no `mergedBy` field | cli/cli#9020, ripgrep#1018 (`pulls/N .merged_by`) | low-med | core + native |
| R3-H3 | ghGetHistoryItem | The commit patches view drops `committer` (and its date) where the summary view shows it | ripgrep `9e79f8c2…` `sections:["patches"]` | low | native |
| R3-H4 | GitHub history | An invalid regex gives "Check the query fields."; the cause is only in `hints` | `matchString:["(unclosed"]`, `regexEngine:"rust"` | low | native + core |
| R3-H5 | ghSearchHistory | A keyword that looks like a qualifier (`author:sharkdp`) gets only "drop filters" on an empty result, not `filters.author` | as given | low | core |
| R3-H6 | ghGetHistoryItem | A compare's default view lists no commits ("commits: 0 of 12 items hasMore") | release compare | low | core |
| R3-H7 | ghCloneRepo | The description still doesn't name "a few paths: ghStructure materialize" | description | low | core |

## Packages (artifactSearch 8.5, securityCheck 8)

| # | Tool | Finding | Evidence | Sev | Owner |
|---|---|---|---|---|---|
| R3-P1 | securityCheck | A package removed from its registry is never sent to OSV, and the refusal reads as a typo (`correctInput`) | PyPI `afriton` (OSV MAL-2024-11514); crates.io `rustdecimal` 1.23.1 (RUSTSEC-2022-0042) | med | native + core |
| R3-P2 | securityCheck | A yanked RubyGems version isn't evaluated, although OSV's ranges cover it | `rest-client` 1.6.13 (CVE-2019-15224; GHSA-333g-rpr4-7hxq) | med | native |
| R3-P3 | artifactSearch | A Go package path inside a module loses owner/repo and gets a warning that contradicts the ref it gives | `github.com/stretchr/testify/assert` v1.9.0; `github.com/gin-gonic/gin/binding` v1.9.1 | low-med | native |
| R3-P4 | artifactSearch | Go vanity modules report `registryRef` although the proxy's tag matches the GitHub tag | `go.uber.org/zap` v1.27.0, `google.golang.org/grpc` v1.62.0, `k8s.io/apimachinery` v0.29.0 | low | native |
| R3-P5 | securityCheck | Linux distro packages never get `upgradeTo` ("not a version this ecosystem's order reads") | Debian 11 openssl `1.1.1n-0+deb11u3`, Alpine, Ubuntu; `security_check/ecosystem.rs:55` | low | native |
| R3-P6 | securityCheck | A version with surrounding spaces is called a range | npm lodash `" 4.17.20 "` | low | native |
| R3-P7 | both | Smaller: no `oversized` flag at `responseChars` 2500; no lookalike lead for `reqeusts`; `@Angular/core` not lowered; bare Maven names get no lead; `github.com/golang/protobuf` not marked deprecated; a `mainGoal` over 500 chars rejects the call | per rater | low | native + core |

## End to end and guidance

| # | Finding | Evidence | Sev | Owner |
|---|---|---|---|---|
| R3-F1 | The instructions name a ghStructure operation `tags` that doesn't exist (`refs`); it is refused, while `branch` → `ref` is repaired | instructions §1 "ghStructure (tree, tags)" | low-med | core |
| R3-F2 | No route for "the first release that contains a change" | serde question needed ghSearchHistory per tag | low | core |
| R3-F3 | A placeholder-empty localSearch row also gets a "No case-sensitive match" hint and a case-insensitive rerun | `\[[A-Z-]+\]` over a file whose only bracketed text is a placeholder | low | native |
| R3-F4 | A cold ghStructure tree of a 9-entry folder took 5.4 s (one observation) | serde `serde_derive/src/de` at 7fc3b4c3 | low | native |
