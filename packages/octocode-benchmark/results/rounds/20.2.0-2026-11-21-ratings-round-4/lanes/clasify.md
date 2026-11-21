# Rate round 4 — clasify lane (lunar-tetra)

Build: `octocode 20.2.0+56f67471`, binary sha256 e246ff42…a405f (packages/octocode-native/npm/darwin-arm64/octocode). CLI `--json` only.
Calls: 39 CLI calls (34 clasify, 5 search-with-classification), 35.9 K output chars. **Billed: 158 requests (budget 40, exceeded by one uncapped search call: 131).**

## Scores

| tool | Desc | Schema | Workflow | Clarity | Time | Tokens | Paging | Smart | Short | Long | Stress | Hints | Overall | Correct |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| clasify | 8 | 8 | 8 | 8 | 9 | 8 | 8 | 8 | 8 | 7 | 9 | 8 | **8** | 8 (25/28 hand-labelled units) |
| `classification` option (localSearch, ghSearchCode) | 6 | 5 | 7 | 7 | 8 | 8 | 8 | 8 | 8 | 6 | 3 | 8 | **6** | 8.5 (precision 22/22 wide, 5/6 + 5/5 narrow; recall 20/22 wide, partial disclosed) |

- clasify: in: localSearch/localFetch/ghGetFileContent/URL → clasify → out: localFetch/ghGetFileContent (next.read), clasify (next.confirm, quiet, details, nextPage)
- classification: in: search query → localSearch/ghSearchCode+classification → out: localFetch (related), localSearch (unfiltered, unfilteredUnknown, nextCandidatePage)

## Ground-truth tally

| Probe | Billed | Result vs hand label |
|---|---|---|
| perItem 4 Rust lines (panics?) | 4 | 3/4: `arr[i]` noul 0.48 (should be yes) |
| localSearch classification, process.rs `Err(` new-error? | 1 | 5/6: missed :403 `Err(process_failure(...))` |
| sections windows.rs (writes bounds?) | 4 | 4/4 (39-88 took(), 120-169 refused()) |
| ghGetFileContent ripgrep Cargo.toml edition (choice) | 1 | e2024 ✓, lines 29-54 include :52 `edition = "2024"` |
| URL rust-2024 edition guide release (choice) | 1 | v185 ✓, cited line 16 `Release version | 1.85.0` |
| source localSearch process.rs, passes existing error? | 1 | ✗: :148 `Err(error)` quiet; :147 (match arm) 0.31 |
| resources glob {cap,shared,stats}.rs Mutex? | 3 | 3/3 |
| ghSearchCode ripgrep `unsafe` classification | 3 | 5/5 kept, lines verified at e8b49b6 via gh api |
| 5-query batch (noul+criteria, choice, score, context+details, negative) | 5 | 5/5 |
| sections package.rs pages 1-3 (network?) | 3 | 3/3 quiet, correct |
| localSearch classification crates `Mutex::new` process-wide? | **131** | 22 kept, all static/process-wide; rg finds 22 real static ones, 20 kept; 2 missed in unjudged/unknown (status partial, disclosed) |

Free paths used: models, 5 refusals with price+confirm (c1, perItem 8, perItem 300, glob 3510 files, perHit), 6 validation errors, sections quiet/details replays, unfiltered, next.read walks (GitHub 5 pages to line 52).

## Defects

1. **high — search `classification` spends without a cap or consent.** Input: localSearch `{"path":"packages/octocode-native/crates","matchString":["Mutex::new"],"regexEngine":"literal","classification":{"predicates":["The hit guards process-wide state."]}}`. Observed: 131 billed requests, 74,799 input tokens, no plan first. Expected: like clasify, a `maxRequests` field and a free `classificationOverBudget` + `next.confirm` above the default. Evidence: ls-class-wide.json `selection.usage.calls:131`; schemas of localSearch/ghSearchCode/ghSearchHistory/structureSearch have no maxRequests; the only bound is `SelectionLimits { max_chars: 131_072 }` (crates/runtime-base/src/execution/limits.rs:57), i.e. chars, not requests (one request per file). Owner: core (field) + native (plan/confirm).
2. **med — R3-C1 partly fixed.** 170 KB source, maxRequests 1: now a price (7 sections, ceiling 12) and `next.confirm`, but still spends 1 request on the whole-window refusal. Cause per crates/tools-shared/src/judgment/windows.rs:1-11: window bounds are learned in process memory only. Owner: native.
3. **med — hit-level judgments attach to the wrong unit when windows overlap.** process.rs :147/:148 (pass-through at :148 scored quiet, the match arm :147 got 0.31); :403 helper-built error missed despite the predicate naming helpers; `arr[i]` 0.48. Owner: native (unit window) / model.
4. **low — cost granularity.** 62 KB file → 29 sections of ~40 lines, one request each (full walk 29 billed); perItem sends one request per short item (300 one-liners → 300 requests; 8 one-liners priced at 512,000 inputTokensAtMost). Owner: native.
5. **low — replay usage.** Replayed pages carry `usage.calls:1` with `replayed:true` (sections details replay, batch pages 2-3): a naive sum double-counts. The details replay of a quiet-only page shows `sections: []` although details should list quiet units. Owner: native.
6. **low — guidance polish.** Score refusal example shows a noul question; `maxRequests:0` example drops maxRequests entirely (leads to the uncapped default); summary `score:1.97` has no scale; glob resource with a yes cites cap.rs 1-113 (broad); `next.unfiltered` lists hits without which were related vs excluded.

## Regression check (round 3)

| id | status | evidence |
|---|---|---|
| R3-C1 | partly fixed | c1-retest: confirm + 7 sections + ceiling 12 now; 1 request still spent |
| R3-C2 | fixed | lines 29-54 hold :52 `edition = "2024"` |
| R3-C3 | fixed | `responseChars:600` → 3 pages with `responsePagination.next`, walked |
| R3-C4 | fixed | `pattern` → example in caller's localSearch form with matchString; http → https example of same url form |
| R3-C5 | partly fixed | `next.quiet` on page 1 now; glob resources still have only per-resource `next.read`, no top-level |
| R3-C6 | fixed | every refusal text "at most N (requestsCeiling)" equals the field; inputTokensAtMost on all 5 refusals |

Not checked: `next.nextCandidatePage` and `next.unfilteredUnknown` walks (billed; stopped after the overrun), images/box, MCP surface.

## Good

Free, fast pricing (3,510-file glob planned in 6.7 s at 0 cost) with an executable confirm; replays of sections pages and `next.read` are free and exact; deciding lines now land on the deciding line.
