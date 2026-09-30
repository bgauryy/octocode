# Local lane: live feature validation (2026-09-30)

Scope: localSearch, localFetch, structureSearch, astSearch, lspSearch, astTopology/astRewrite (beta), `octocode graph`, plus local cross-cutting features (minimal vs debug, pagination/followUp, responsePagination, shared/base, bulk, defaultExcludes, path sandbox, secret redaction).

- CLI: `octocode 19.2.0 (native 20.0.0)`, cwd `/Users/bgaryy/code/octocode`, no ALLOWED_PATHS/WORKSPACE_ROOT set unless stated.
- Fixture: `probes/local/fx/` (FX below) = `src/{a,b,index}.ts`, `src/manyhits.txt` (30 hits), `src/big.txt` (300 lines/21 KB), `src/huge.txt` (92 KB), `src/lib.rs`, `many/f01..f25.txt`, `node_modules/ dist/ target/ secrets/ .github/ .hidden/`, `sec/creds.ts` (fake AKIA/ghp_/PEM), `sec/aws2.py`, `sec/.env`, symlinks `sec/hosts_link→/etc/hosts`, `sec/ssh_link→~/.ssh`, `sec/etc_dir_link→/etc`, `rw/r.ts` (astRewrite apply target).
- Raw outputs: `probes/local/<probe>.out` (each file begins with the exact command JSON, env, exit code). Runner: `probes/local/run.py`; MCP scripts `probes/local/mcp1.mjs` (log `mcp1.log`, `mcp1.json`), `probes/local/mcp2.mjs`.
- Every query carried `goal:"g",reasoning:"r"` unless the probe tests their absence.
- MCP availability: `mcp1.mjs` ran OK. MCP was then blocked for about 15 min by a contract fingerprint mismatch while another agent rebuilt native (core 872ab3bb… != native 2a388c88…; `OCTOCODE_ALLOW_CONTRACT_DRIFT=1` is ignored by the dist build). `mcp2.mjs` was rerun after the drift cleared and succeeded.

## Feature table

Counts (90 rows): VERIFIED 70 · PARTIAL 14 · FAILS 6 · UNTESTABLE 0.

| # | Feature | Documented | Probe (file in probes/local) | Status | Evidence |
|---|---|---|---|---|---|
| 1 | localSearch literal | OCTOCODE_TOOLS.md:657 | ls_literal: `localSearch {"path":FX/src,"searchText":"alphaNeedle(","regex":"literal"}` | VERIFIED | a.ts matches line 3 and line 11; exit 0 |
| 2 | Rust regex + matchOnly unique:list | :657, :643-644 | ls_regex: `searchText:"export\\s+(function\|class)\\s+\\w+", regex:"rust", resultView:"matchOnly", unique:"list"` | VERIFIED | `export function alphaNeedle`, `export class Widget`, `export function helper` with columns |
| 3 | pcre2 lookbehind | :657 | ls_pcre2: `"(?<=const )y", regex:"pcre2"` | VERIFIED | match line 4 `const y = helper(x);` |
| 4 | caseMode smart/sensitive/insensitive | :658 | ls_case_*: WIDGET sensitive / WIDGET insensitive / Widget smart | VERIFIED | sensitive: 0, exit 1, status empty. insensitive: a.ts 1, index.ts 2. smart+uppercase: same as sensitive-exact |
| 5 | wholeWord | :659 | ls_wholeword: `"value"` in `value_N` | VERIFIED | 0 matches (`_` is a word char), exit 1 |
| 6 | multiline dotall | :660 | ls_multiline: `helper.*\n.*return`, dotall, countMatches | VERIFIED | b.ts totalOccurrences 1 |
| 7 | invertMatch (lines) | :662 | ls_invert_default: `console` invertMatch on b.ts | VERIFIED | returns the 5 non-console lines |
| 8 | invertMatch + resultView:"files" = "files lacking the pattern" | :662 | ls_invert_files: `console`, invertMatch, files, include `*.ts` | FAILS | returned `a.ts, b.ts, index.ts`, and a.ts/b.ts contain `console`. filesWithout (ls_filesWithout_ts) correctly returns only `index.ts` |
| 9 | contextLines / detailed default 3 | :645 | ls_context2 (regex literal, contextLines 2); ls_detailed | VERIFIED | value spans lines 2-6 around line 4; detailed `matchLines:[1,4]` in one merged window |
| 10 | matchContentLength clipping | :646 | ls_matchlen: matchContentLength 10 | VERIFIED | `"row 000...","truncated":true,"originalChars":69,"returnedChars":10` + warning |
| 11 | langType / include / exclude | :668-670 | ls_langType, ls_include | VERIFIED | ts gives only a.ts and index.ts. include `*.txt` + exclude `f1*` drops f10-f19 |
| 12 | filesWithout | :611 | ls_filesWithout | VERIFIED | big.txt, index.ts, manyhits.txt |
| 13 | sort path + reverse | :673-674 | ls_sort_path_rev (pageSize 3) | VERIFIED | f25, f24, f23; totalPages 9 |
| 14 | unique:count, matchWindow | :643-644 | ls_unique_count, ls_matchWindow | VERIFIED | `value_1` count 11; `"…edle value_10"` |
| 15 | maxDepth 0 = root files only | schema | ls_maxdepth0 on FX | VERIFIED | empty; only package.json at root and it has no needle |
| 16 | File pagination page/pageSize + next.nextPage with followUp (CLI) | :585-590, TOOL_DATA_CONTRACT.md:118 | ls_page1, then ls_page2_follow/ls_page3_follow, each running next.query unchanged | VERIFIED | p1 exit 6 hasMore, next query `{snapshot,page:2,...,"followUp":true}` with no goal. p3 `hasMore:false`, exit 0, no next |
| 17 | Per-file matchPage | :647-648, :701 | ls_matchpage1 then 2 then 3 via next.nextMatchPage | VERIFIED | manyhits 30 matches as 10/10/10. Later pages list only manyhits.txt. `moreLines` hint |
| 18 | Stale snapshot gives restart | :702 | ls_page2_stale (added f26 after page 1) | VERIFIED | `errorCode:"staleSnapshot"`, next.restart (no page/snapshot, followUp:true), exit 5 |
| 19 | New query without goal/reasoning rejected | TOOL_DATA_CONTRACT.md:118 | ls_nogoal (CLI); mcp_ls_nogoal | VERIFIED | CLI exit 2 "Missing required field: goal (... only a next.* continuation with followUp: true inherits it)". MCP isError |
| 20 | followUp only for continuations | TOOL_DATA_CONTRACT.md:118 | ls_followup_handwritten: hand-written `{followUp:true,path,searchText}` with no goal | PARTIAL | accepted and executed (exit 6). Any caller can skip goal/reasoning by adding followUp:true |
| 21 | MCP continuation run unchanged | TOOL_DATA_CONTRACT.md:111-118 | mcp1.mjs: mcp_ls_page1, then `raw('localSearch',{queries:[next.nextPage.query]})` | VERIFIED | page 2 returns f11… with isError false |
| 22 | base + relative paths | :695, TOOL_DATA_CONTRACT.md:132 | every localSearch probe | VERIFIED | `base` = queried dir (or parent of a file); row paths relative |
| 23 | Binary NUL handling | :684 | ls_binary (file with NUL on line 2) | PARTIAL | isPartial, terminalLimit, binaryFileSkipped warning present. `capped`/`capReason:"binaryQuit"` only appear with debug (stats is debug-only) |
| 24 | Missing path | :683 | ls_missing_path | VERIFIED | `pathNotFound`, exit 3 |
| 25 | defaultExcludes (localSearch) prunes deps/build/cred/.github | :671 | ls_basic, ls_hidden, ls_nodefex, ls_nodefex_noignore | PARTIAL | Default + hidden: only `.hidden/h.txt`. defaultExcludes:false adds `.github` only, because node_modules/dist/target stay hidden by .gitignore until noIgnore:true. `secrets/s.txt` never appears, even with defaultExcludes:false + noIgnore + hidden |
| 26 | defaultExcludes (structureSearch tree) | :808 | ss_tree_hidden, ss_tree_nodefex | PARTIAL | default+hidden: `.github/ .hidden/ many/ sec/ src/`. `.github` stays visible, as documented. defaultExcludes:false adds dist/ node_modules/ target/, but `secrets/` is still absent. Schema text says "false also walks … credential dirs" |
| 27 | structureSearch tree + pagination | :786-811 | ss_tree (maxDepth 1), ss_tree_page (pageSize 10) | VERIFIED | entries with sizes and summary "36 entries". nextPage with snapshot and followUp |
| 28 | structureSearch files: names/pathRegex/size/detail/sort/time/empty/page | :827-857 | ss_files_* | VERIFIED | detail full → `size, modifiedMs, lineCount`; sort size; `pathRegex ^f0[1-3]\.txt$` → 3; modifiedWithin 1h; nextPage present. Note: the root dir itself is an entry (`{"path":"many","type":"directory"}`), so totalFiles is 26 for 25 files |
| 29 | localFetch path-only read | :864, :880 (chunkSize default 100 lines) | lf_path_only_big, lf_default_chunk_dbg | PARTIAL | Returns 234 lines of big.txt and 178 of huge.txt (the 16 KiB budget), not 100. next.continue chunkSize:234 / 178. Paging works: p2 = lines 235-300, exit 0 |
| 30 | localFetch lines chunking offset/chunkSize | :878-880 | lf_chunk_lines offset 100, chunkSize 50 | VERIFIED | sourceLineRanges 101-150, nextOffset 150, exit 6 |
| 31 | localFetch bytes chunking | :878 | lf_chunk_bytes, then lf_chunk_bytes_p2 via next.continue | VERIFIED | 200 B pages; p2 starts mid-line 3 at offset 200 |
| 32 | Offset past end | :887 | lf_offset_past, lf_offset_past_dbg | PARTIAL | empty content + warning + `next.restart` offset 0, exit 0. `pagination.outOfRange:true` only with debug |
| 33 | matchString + contextLines merge; matchedLines | :881-882, :891 | lf_match (`helper`, ctx 1) | VERIFIED | sourceLineRanges 1-5 (merged), matchedLines [1,4] |
| 34 | matchString regex + omitted marker | :881 | lf_match_regex `alpha\w+\(` ctx 0 | VERIFIED | `... [lines 4-10 omitted] ...`, matchedLines [3,11] |
| 35 | contextBytes on byte chunks | :883 | lf_match_bytes `row 0150`, contextBytes 20 | VERIFIED | 48-char window, matchedLines [150] |
| 36 | matchString no hit | — | lf_match_none | VERIFIED | status empty, errorCode noMatches, exit 1 |
| 37 | fullContent small / >50000 B bounded recovery | :885, :893 | lf_fullcontent, lf_fullcontent_big (21 KB), lf_fullcontent_huge (92 KB) | VERIFIED | 92 KB → partialReasons `full-content-size-limit` + next.continue, exit 6 |
| 38 | Selector exclusivity | :864, :884-885 | lf_full_chunk_conflict, lf_range_match_conflict, lf_symbols_range_conflict | VERIFIED | exit 2 with precise details for each |
| 39 | minify standard / symbols / match forces none | :884, :891 | lf_minify_standard, lf_minify_symbols, lf_minify_std_match | VERIFIED | contentView standard. symbols `N\| ` prefixed outline. `minifyFallback:{requested:"standard",applied:"none",reason:"match-evidence"}` |
| 40 | Every read reports totalLines + sourceBytes | :889 | min_lf vs dbg_lf | PARTIAL | minimal has only totalLines. sourceBytes/returnedBytes/modified are debug-only |
| 41 | localFetch on a directory | :1065 | lf_dir | VERIFIED | fileAccessFailed "Path is not a regular file", exit 5 |
| 42 | astSearch match pattern + $$$ capture span/count + expandCaptures | :744-770 | min_asm `console.log($$$A)` | VERIFIED | b.ts capture `{"count":3}` + `capturesTruncated:true`, next.expandCaptures (followUp) |
| 43 | astSearch YAML rule (inside/stopBy) | :780 | ast_rule | VERIFIED | only `console.log('run')` inside method_definition |
| 44 | astSearch syntaxTree paging | :712-714 | ast_syntaxTree nodeLimit 5, then next.nextPage | VERIFIED | totalNodes 30; p2 starts at node id 5 |
| 45 | astSearch symbols (file / directory) | :747-760 | ast_symbols_file, ast_symbols_dir_repair | VERIFIED | `{name,kind,line,endLine,exported,parent}`; directory → `files:[{path,declarations}]` |
| 46 | astSearch symbols directory with langType | LOCAL_RESEARCH_WORKFLOW.md:22 | ast_symbols_dir (`langType:"typescript"`) | PARTIAL | error `ast.language.fileRequired` exit 5 + next.repair (drop langType). Not documented |
| 47 | Unknown YAML kind gives typed compile diagnostic | :741-744 | ast_badkind | VERIFIED | `structural.query.compileFailed` "unknown node kind", exit 5 |
| 48 | Semicolon / return-type retry with `structural.query.rewritten` | :732-737 | ast_rewritten(_dbg) `const y = helper(x)`, ast_rewrite_return `return y + 1`, ast_rewrite_ret_type | FAILS | all 0 matches, only `structural.query.noMatches`. No "rewritten" code exists in runtime ast_search (grep) |
| 49 | Rust `fn $N()` visibilityExact diagnostic | :765-768 | ast_rust_fn, ast_rust_fn_dbg | PARTIAL | matches only `fn hidden`. Diagnostic appears only with debug:true (info-level), so minimal output gives no hint that `pub fn visible` was excluded |
| 50 | lspSearch definition (symbolName+lineHint) | :1172 | lsp_def | VERIFIED | b.ts line 1 with content |
| 51 | lspSearch references (+groupByFile) | :1173 | lsp_refs, lsp_refs_group, lsp_repo_refs (repo file validationMessages.ts:42) | VERIFIED | 3 refs / 2 files. byFile `{path,references,lines}`. Repo: 2 refs. Rows carry undocumented `source:"recoveredImporter"` |
| 52 | lspSearch position anchor (hover) | :1156, :1177 | lsp_pos_hover `{line:3,character:12}` | PARTIAL | works, but `payload.hover.range` is raw 0-based `{line:3,character:12}` next to 1-based foundAtLine 4 (see drift D9) |
| 53 | callers / callees | :1174-1175 | lsp_callers, lsp_callees | VERIFIED | `run` (Widget) calls alphaNeedle at 11:5; callee `helper` |
| 54 | workspaceSymbol via uri / workspaceRoot | :1180 | lsp_wsym_uri, lsp_wsym_root | VERIFIED | items + honest `partialReasons:["inferredProject"]` + next.textSearch |
| 55 | position + symbolName mutually exclusive | :1156 | lsp_both_anchor | VERIFIED | exit 2 "mutually exclusive" |
| 56 | Out-of-range position / bad lineHint gives lsp.anchorUnresolved + recovery | :1264-1266 | lsp_bad_pos, lsp_wrong_line | VERIFIED | exit 5; next.readFile (matchString when a name was given). Message says "it has 14 lines" for a 13-line file |
| 57 | lsp pagination + snapshot + restart | :1216-1223 | lsp_repo_docsym_p1 (pageSize 5, 46 syms), p2 via nextPage; p2 without snapshot; forged snapshot | VERIFIED | p2 correct. Missing or forged snapshot → `lsp.snapshot.changed` + next.restart, exit 5 |
| 58 | documentSymbols native fast path preferred even with a server | :1326-1331 | dbg_lsp on FX/src/a.ts | FAILS | `lsp.source:"lsp"`, provider documentSymbolProvider, typescript-language-server receipt |
| 59 | LSP root: files inside WORKSPACE_ROOT use it | :1318-1322 | lsp_fx_wsroot_env (WORKSPACE_ROOT=/Users/bgaryy/code/octocode, debug) | FAILS | receipt.workspaceRoot = FX (nearest package.json), not WORKSPACE_ROOT |
| 60 | Minimal by default vs debug:true | TOOL_DATA_CONTRACT.md:72-85 | min_* / dbg_* (7 tools, same query ± debug) | VERIFIED | bytes min→dbg: ls 441→641, lf 362→507, sst 308→451, ssf 309→559, asm 1088→1812, asy 451→621, lsp 661→1802. Debug adds meta.evidence, stats, snapshot, finished-page pagination, searchEngine, diagnostics, lsp receipt. Nothing is removed. Open pagination and next are kept in minimal |
| 61 | Error rows complete in minimal | TOOL_DATA_CONTRACT.md:84 | lsp_bad_pos, sb_* | VERIFIED | error rows keep type/lsp/hints/next |
| 62 | responsePagination (responseCharLength/Offset/Snapshot) | TOOL_DATA_CONTRACT.md:112, :123-125 | rp_page1 (`responseCharLength:300`), then rp_page2 = responsePagination.next.query as args | VERIFIED | results:[] + responseWindow "Response page 1/2"; p2 hasMore false, exit 0 |
| 63 | responseSnapshot restart on change | TOOL_DATA_CONTRACT.md:123 | rp_stale_p1, append line, rp_stale_p2 | VERIFIED | `changed:true, restart:true`, next offset 0 without snapshot |
| 64 | `shared` hoisting | TOOL_DATA_CONTRACT.md:134 | mcp_ss_raw (MCP), ss_files_regex, beta_tp_beta1 | VERIFIED | `shared:{"lineCount":1}`, `{"sizeFormatted":"17B"}`, topology `{"confidence":"syntactic","importLine":1,...}` |
| 65 | Bulk: 1-5 ok, 6 rejected, 0 rejected | TOOL_DATA_CONTRACT.md:44, :591 | bulk_6, bulk_0, mcp_ls_6q | VERIFIED | CLI exit 2 "length 6 exceeds the maximum of 5". MCP isError "Too big: expected array to have <=5 items" |
| 66 | Bulk mixed success/error/empty, order preserved | TOOL_DATA_CONTRACT.md:66 | bulk_mixed (5 rows), bulk_ls_mixed | VERIFIED | indices 0..4 = ok, pathOutsideAllowedRoots, fileAccessFailed, ok, empty/noMatches. CLI exit 0 |
| 67 | Per-row goal/reasoning required | TOOL_DATA_CONTRACT.md:44 | bulk_row_missing_goal (CLI); mcp_bulk_missing_goal_row (MCP) | VERIFIED | Both surfaces run row 0 and return row 1 as `invalidInput` "queries.1.goal: Missing required field" (CLI exit 2, MCP isError:false). A batch where every row lacks goal is rejected whole (CLI toolError exit 2 / MCP SDK input-validation error) |
| 68 | Sandbox: outside root /etc/hosts | :565, SECURITY.md:55 | sb_lf_etc_hosts, sb_ls_etc, sb_lsp_etc, sb_ast_etc, sb_ss_etc | VERIFIED | `pathOutsideAllowedRoots` exit 5 (ast: `ast.policy.outsideAllowedRoots`; structureSearch: `structure.policy.outsideAllowedRoots`, which is undocumented) |
| 69 | Sandbox: symlink escape | :565, SECURITY.md:57 | sb_lf_symlink_hosts/_ssh/_etcdir, sb_ast_symlink, sb_lsp_symlink | VERIFIED | "Symlink target 'hosts' is outside allowed directories", exit 5. ast: `ast.policy.symlinkEscape`. localSearch/tree skip symlinks silently |
| 70 | Sandbox: ../ traversal | SECURITY.md:55 | sb_lf_dotdot_abs, sb_lf_dotdot_rel, sb_lf_dotdot_inside | VERIFIED | escapes denied exit 5; `src/../src/a.ts` inside root normalized and read |
| 71 | Home dir allowed by default | OCTOCODE_TOOLS.md:563, SECURITY.md:53, README.md:444 | sb_lf_sibling_repo (`~/code/octocode-mcp-host/package.json`), sb_lf_ssh_pub | FAILS | denied: "outside allowed directories (allowed: /Users/bgaryy/code/octocode, /Users/bgaryy/.octocode)". Runtime hardcodes `include_home:false` (runtime/src/runtime/engine.rs:375; test `home_is_not_allowed_unless_opted_in`) |
| 72 | ALLOWED_PATHS adds roots | :563 | ap_home_sibling (ALLOWED_PATHS=$HOME) | VERIFIED | sibling repo readable |
| 73 | Sensitive files denied even when allowed | SECURITY.md:56, :60 | ap_home_ssh_key/pub/search, lf_dotenv, lf_secrets_dir | VERIFIED | "is in an ignored directory or matches an ignored pattern", fileAccessFailed exit 5 (`~/.ssh`, `.env`, `secrets/`) |
| 74 | ALLOWED_PATHS=/etc | — | ap_etc_hosts, ap_private_etc_shells/hosts | PARTIAL | still denied as "ignored pattern" (system dirs are deny-listed). Not in the documented sensitive classes |
| 75 | WORKSPACE_ROOT resolves relative paths | OCTOCODE_TOOLS.md:562, README.md:373, :444 | ws_rel (cwd repo), ws_rel_cwd_tmp (cwd /private/tmp), ws_rel_cwd_fx_noenv | FAILS | WORKSPACE_ROOT=FX, `path:"src/a.ts"` gives pathOutsideAllowedRoots, `resolvedPath:"src/a.ts"`. Works only when cwd=FX. `absolutize()` joins process cwd (policy/path.rs:416-425) |
| 76 | Denied errors use safe relative paths | SECURITY.md:60 | sb_lf_ssh_key, sb_lf_sibling_repo, lf_secrets_dir | PARTIAL | outside-root errors echo absolute input and list absolute allowed roots. `resolvedPath` is absolute on sensitive denials. The sensitive-denial message itself is relative / `~` |
| 77 | Redaction: AKIA / ghp_ / PEM in localFetch | SECURITY.md:33-35, :893 | red_lf_full, red_lf_match_pem_mid (lines 7-8), red_lf_bytes_pem, red_lf_std, red_lf_symbols | VERIFIED | `[REDACTED-AWSACCESSKEYID]`, `[REDACTED-GITHUBTOKENS]`, `[REDACTED-PRIVATEKEYFRAGMENT]` + "Redacted private-key block(s) found in the source before selecting the window". Mid-PEM range and byte page don't leak |
| 78 | Redaction in localSearch snippets / matchOnly / astSearch captures | :685 | red_ls, red_ls_matchonly, red_ast_semicolon | VERIFIED | `redactedMatches` warning. Captures `"[REDACTED-AWSACCESSKEYID]"`, PEM → `[REDACTED-RSAPRIVATEKEY]` |
| 79 | Redaction of AWS secret access key / high-entropy | README.md:442 | red_lf_full vs red_lf_aws2 | PARTIAL | `aws_secret_access_key = "Zq8v…"` is redacted. `awsSecret = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"` (40-char, no key-name context) leaks in localFetch/localSearch/astSearch |
| 80 | matchString cannot match secret text | :883 | red_lf_match_akia | VERIFIED | empty + hint "matchString runs on [REDACTED…] placeholders, never secret text" |
| 81 | localSearch with a secret-shaped searchText | — | red_ls_searchtext_secret (literal ghp_… present in file) | PARTIAL | status empty, generic "No matches" hint, nothing says the query was redacted. Misleading absence |
| 82 | astRewrite / astTopology gated by OCTOCODE_BETA (CLI) | README.md:210-212, SECURITY.md:62, :1071 | beta_rw_nobeta, beta_tp_nobeta, beta_*_beta1, beta_*_betatrue | VERIFIED | unset → `{"error":"… is a beta feature…","errorCode":"missingConfiguration"}` exit 5. `=1` and `=true` both enable |
| 83 | astRewrite preview is read-only, returns next.apply with hashes | :1071-1086 | beta_rw_beta1 | VERIFIED | patch diff, beforeHash/afterHash, next.apply with snapshot+expectedHashes. a.ts unchanged afterwards |
| 84 | astRewrite apply guards (scratch only) | :1081, :1098 | rw_apply_badhash, rw_apply, rw_apply_replay, rw_apply_nobeta (fx/rw/r.ts) | VERIFIED | bad hash → `ast.rewrite.hash_mismatch` (file untouched); apply committed; replay → `ast.rewrite.snapshot_changed`; no beta → missingConfiguration |
| 85 | astTopology dependencies (beta) | :906-1000 | beta_tp_beta1 (`analysis:"dependencies", file:"src/index.ts", depth 2`) | VERIFIED | a.ts d1, b.ts d2, edgeKinds static-import, immediateDominator, completeness complete |
| 86 | MCP gating: astTopology only with OCTOCODE_BETA; astRewrite never on MCP | README.md:217, SECURITY.md:62, CONFIGURATION.md:353 | mcp2.mjs (log mcp2.log) | VERIFIED | no beta: 13 tools, calls to astTopology/astRewrite → `-32602 Tool … not found`. OCTOCODE_BETA=1: 14 tools (+astTopology; cycles call OK). astRewrite still `not found` |
| 87 | graph ingest (scratch) + reuse | `octocode graph --help` | graph_ingest, graph_reingest (`graph ingest . --workspace FX`) | VERIFIED | 6 files, 21 nodes / 22 edges, callInternalRecall 1.0; second run `reused:true` |
| 88 | graph query stats/find/callers/dependents/impact/issues/cycles | help | graph_stats, graph_find, graph_callers, graph_dependents2, graph_impact, graph_issues | VERIFIED | callers of `src/b.ts#helper` = alphaNeedle via import, high. dependents src/b.ts = src/a.ts static-import. impact willBreak a.ts, entrypoint index.ts |
| 89 | graph stale | help | graph_stale0, then append to index.ts, graph_stale1 | VERIFIED (with note) | fresh:true, then `{"file":"src/index.ts","status":"changed"}`. The `reingest` hint drops `--workspace` (it would write into the repo-root .octocode) |
| 90 | graph exit codes 1 empty / 3 not found | help | graph_find_empty, graph_nonode, graph_nograph | VERIFIED | exit 1; exit 3 graph.nodeNotFound; exit 3 graph.notFound. Zero-result `cycles` exits 0 (inconsistent with find) |

## Doc drift

- D1. OCTOCODE_TOOLS.md:563, SECURITY.md:53, README.md:444 say the home directory is always allowed by default. In fact the defaults are only the workspace (cwd) and `~/.octocode`; `include_home:false` is hardcoded at engine.rs:375. Sibling repos under home are denied without ALLOWED_PATHS.
- D2. OCTOCODE_TOOLS.md:562, README.md:373/:444, CONFIGURATION (table) say WORKSPACE_ROOT resolves relative local paths. In fact relative paths resolve against the process cwd. WORKSPACE_ROOT only replaces cwd as the allowed root, so a relative path under WORKSPACE_ROOT is denied. (Also DEFECT-1.)
- D3. OCTOCODE_TOOLS.md:732-737 describe a semicolon / return-type retry with `structural.query.rewritten`. It doesn't happen, and there is no implementation in runtime ast_search.
- D4. OCTOCODE_TOOLS.md:1326-1331 say documentSymbols prefers the native oxc fast path even when a server is present. In fact `lsp.source:"lsp"` via typescript-language-server whenever a server is available.
- D5. OCTOCODE_TOOLS.md:1318-1322 say files inside WORKSPACE_ROOT use that root for LSP. In fact the nearest project marker (package.json) wins even when WORKSPACE_ROOT is set and contains the file.
- D6. OCTOCODE_TOOLS.md:662 says `invertMatch` + `resultView:"files"` gives "files lacking the pattern". In fact it gives files with at least one non-matching line (rg -v -l semantics); use `filesWithout`.
- D7. OCTOCODE_TOOLS.md:880 says chunkSize defaults to 100 lines. A path-only read returns up to the 16 KiB budget (234 lines of big.txt, 178 of huge.txt) and the continuation carries that chunkSize.
- D8. OCTOCODE_TOOLS.md:889 says every successful read reports `totalLines` and `sourceBytes`, and :887 says `pagination.outOfRange:true`. In minimal output `sourceBytes`/`returnedBytes` are absent, and `pagination.outOfRange` appears only with debug. This is consistent with TOOL_DATA_CONTRACT "Minimal by default", but OCTOCODE_TOOLS doesn't mention it.
- D9. OCTOCODE_TOOLS.md:1227 says every emitted coordinate is one-based. `payload.hover.range` is the raw LSP 0-based range `{line:3,character:12}`, while `resolvedSymbol.foundAtLine` is 4.
- D10. TOOL_DATA_CONTRACT.md:101 and LOCAL_RESEARCH_WORKFLOW.md:30 say to inspect `data.lsp.source`. In minimal output the whole `lsp` object is dropped; `lsp.source` is debug-only. TOOL_DATA_CONTRACT:79 names only the "receipt and workspaceRoot" as debug-only. OCTOCODE_TOOLS.md:1185-1200 lists `operation`, `uri`, and `lsp` in the "all semantic responses" envelope; none of them are in minimal documentSymbols output.
- D11. OCTOCODE_TOOLS.md:767 says the Rust `fn` pattern "adds a `structural.pattern.visibilityExact` info diagnostic". It is visible only with debug:true (info diagnostics are debug-only), so default users get a silent partial answer.
- D12. OCTOCODE_TOOLS.md:684 says the binary file result has `capped:true` and `capReason` with `binaryQuit`. Those fields are only in debug stats; minimal has the warning, isPartial, and terminalLimit.
- D13. The `defaultExcludes` schema text ("false also walks dependency, build, cache, and credential dirs") and OCTOCODE_TOOLS.md:671/:808 don't match behavior. `secrets/` is never walked, even with defaultExcludes:false (+noIgnore) because the sensitive-path policy wins. In localSearch, node_modules/dist/target also stay hidden by .gitignore unless noIgnore:true.
- D14. OCTOCODE_TOOLS.md:565 lists `pathOutsideAllowedRoots` codes per tool. structureSearch returns the undocumented `structure.policy.outsideAllowedRoots`.
- D15. SECURITY.md:60 says denied errors use safe relative paths. Outside-root denials echo the absolute input path and list absolute allowed roots; sensitive denials include an absolute `resolvedPath`.
- D16. README.md:377 and CONFIGURATION.md table say OCTOCODE_BETA is "the sole gate for the `astRewrite` tool". It also gates astTopology (CLI error "astTopology is a beta feature", as README.md:211 says). The OCTOCODE_TOOLS.md astTopology section (:904+) doesn't mention the beta gate at all.
- D17. OCTOCODE_TOOLS.md:1132 documents `source:"recoveredAlias"`. References now also carry the undocumented `source:"recoveredImporter"`.
- D18. OCTOCODE_TOOLS.md:576 says "`localFetch` is pure Node.js". It runs in the native runtime (stale sentence).
- D19. TOOL_DATA_CONTRACT.md:80 says request echoes (`type`) are debug-only. Minimal workspaceSymbol rows still carry `"type":"workspaceSymbol"`.
- D20. LOCAL_RESEARCH_WORKFLOW.md:22 says "`langType` for directory searches" without scoping it to match. `symbols` on a directory rejects langType (`ast.language.fileRequired`).
- D21. SECURITY.md sensitive classes don't mention system directories. With ALLOWED_PATHS=/etc (or /private/etc), `/etc/hosts` and `/etc/shells` are still denied as "ignored pattern".

## Defects

- DEFECT-1 (High). WORKSPACE_ROOT does not resolve relative paths. Repro: `WORKSPACE_ROOT=$FX octocode localFetch '{"goal":"g","reasoning":"r","path":"src/a.ts","startLine":1,"endLine":1}'` from any cwd other than FX gives `pathOutsideAllowedRoots`, `resolvedPath:"src/a.ts"`, exit 5. Cause: `absolutize()` joins `std::env::current_dir()` (runtime/src/policy/path.rs:416-425), not the workspace root. Reproduced on MCP too: server started with WORKSPACE_ROOT=FX, cwd=repo; `localFetch path:"src/a.ts"` → pathOutsideAllowedRoots (mcp2.log `mcp_ws_rel`). This breaks MCP hosts that set WORKSPACE_ROOT but launch from another cwd. (probes ws_rel, ws_rel_cwd_tmp, mcp_ws_rel)
- DEFECT-2 (Medium, doc-contract). The home directory is not an allowed root, although three docs promise it (D1). Either fix the docs or wire `include_home`. (sb_lf_sibling_repo)
- DEFECT-3 (Medium). `invertMatch:true` + `resultView:"files"` lists files that do contain the pattern. The documented meaning is "files lacking the pattern", so an agent can conclude absence wrongly. (ls_invert_files vs ls_filesWithout_ts)
- DEFECT-4 (Medium, doc/impl gap). The semicolon / return-type auto-retry and the `structural.query.rewritten` diagnostic are missing; zero-match results for `const y = helper(x)` and `return y + 1` (ast_rewritten, ast_rewrite_return).
- DEFECT-5 (Medium, usability/honesty). Minimal output hides `structural.pattern.visibilityExact`, so `fn $N()` silently omits `pub fn` items with no hint (ast_rust_fn). The same class of issue applies to `lsp.source` being dropped: an agent can't tell semantic from native evidence without debug (min_lsp).
- DEFECT-6 (Low). `followUp:true` on a hand-written query bypasses the goal/reasoning requirement (ls_followup_handwritten, exit 6).
- DEFECT-7 (Low). localSearch with secret-shaped `searchText` returns a generic empty "No matches" although the file contains the text. localFetch gives an explicit redaction hint in the same case (red_ls_searchtext_secret vs red_lf_match_akia).
- DEFECT-8 (Low). An unlabeled 40-char AWS secret key (`"wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"`) is not redacted in localFetch/localSearch/astSearch. It is caught with an `aws_secret_access_key =` label (red_lf_full vs red_lf_aws2). The README claims high-entropy detection.
- DEFECT-9 (Low). `hover.range` is 0-based while every other emitted coordinate is 1-based (lsp_pos_hover).
- DEFECT-10 (Low). `graph query stale` suggests `reingest: "octocode graph ingest <root>"` without the `--workspace` used for the snapshot. Following it would write a new snapshot into the nearest .git ancestor's `.octocode/graph` (graph_stale1).
- DEFECT-12 (Low). Beta-gate errors use a bare `{"error","errorCode":"missingConfiguration"}` without the `kind:"octocode.toolError"` envelope that other CLI validation errors use (beta_rw_nobeta).
- DEFECT-13 (Low). lsp anchorUnresolved says "it has 14 lines (0-based lines 0-13)" for a 13-line file with a trailing newline (lsp_bad_pos).
- DEFECT-14 (Low). structureSearch `operation:"files"` without entryType includes the root directory itself as a result row, so totalFiles is 26 for 25 files (ss_files_page).
- Harness note (not product): `harness/mcp-client.mjs` waits the full 240 s timeout when the server exits during init (fingerprint mismatch) instead of failing fast on process exit.

## MCP cross-check (mcp1.log / mcp2.log)

MCP matched the CLI on every repeated probe: continuation replay, the 6-query rejection, mixed bulk (isError:false with per-row errors), responsePagination, redaction (localFetch + localSearch), sandbox /etc/hosts + symlink (isError:true, pathOutsideAllowedRoots), the WORKSPACE_ROOT relative-path failure (DEFECT-1), and beta gating. MCP text renders `content (source lines): N:` line prefixes, or `content (copy-safe)` when redacted. structuredContent keeps raw content.
