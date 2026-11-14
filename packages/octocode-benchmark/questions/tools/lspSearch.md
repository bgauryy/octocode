# lspSearch questions

These are capability questions, not answer keys. Record exact source hashes, build identity,
schema, calls, failed attempts and every required continuation in the campaign. Apply
[shared checks](../../CHECKS.md), [ratings](../../RATINGS.md) and
[comparison accounting](../../COMPARISONS.md). Answers and independent incidence tables belong
in the campaign, never in this question file. Run valid and invalid calls on the pinned Node CLI
and a freshly started newline-framed MCP session. No all-language claim follows from this scope.

The frozen `semantic/inputs/fixture.json` for the 2026-11-14 tool-rating campaign contains the
source and SHA-256 of each file: JavaScript core/barrel/service/entry, shadows, an isolated
function, a two-module cycle, and TypeScript `Shape`, `Circle`, `NamedCircle`, a typed variable
and a Unicode-containing line. Materialize it in an isolated allowed scope. The second scope is
`pallets/itsdangerous` at `096c8d42545d3b68ea21a4f890fb2b2d8979c0bd`, limited to
`src/itsdangerous/*.py`; preserve its license and source hashes. Freeze questions before output.

| ID | Question and requested evidence | Coverage |
|---|---|---|
| LSP-01 | Which declaration does `compute` in `service.js:2` resolve to through its import and re-export? Return a source location usable for a read. | definition; content/files |
| LSP-02 | Enumerate every semantic reference to exported `core` at `core.js:1`, including renamed references. Which are declarations, imports and calls? | references; declaration toggle; content/files |
| LSP-03 | Who calls that exported function, and what does `entry` call? Distinguish caller nodes from call sites and direct from transitive depth. | callers; callees; depth |
| LSP-04 | Where do the two `core` occurrences in `shadow.js:1` and the nested `core` in line 2 resolve? Do comments, strings and unrelated same-name bindings contaminate LSP-02? | occurrence 0/1; identity |
| LSP-05 | What is the type declaration for `shape` in `types.ts:4`, which class implements `Shape`, and what are the supertype/subtype relations of `NamedCircle`/`Circle`? | typeDefinition; implementation; supertypes; subtypes |
| LSP-06 | What type/signature does hover report for the call after the emoji and combining character in `types.ts:5`? Are its coordinates usable to recover the exact token? | hover; Unicode coordinates |
| LSP-07 | List the declarations in `types.ts`, then locate `Circle` by workspace symbol search. Preserve nested symbols and identity. | documentSymbols; workspaceSymbol |
| LSP-08 | What diagnostics does the checked project report for `service.js`? How does that affect completeness wording on a successful reference/definition answer? | diagnostic; readiness; partial coverage |
| LSP-09 | What references remain for the disconnected function with declarations excluded? What happens for a nonexistent workspace symbol? | legitimate empty states |
| LSP-10 | Reconstruct LSP-02 with `limit:1` and the smallest supported response window. Follow response pages before result pages, unchanged, including across CLI/MCP. Does the set equal the unpaged direct-server oracle exactly once? | pagination; mixed batch; fresh process |
| LSP-11 | How are missing line, line zero, a string line, wrong occurrence, caller `contextLines`, foreign `query` and incompatible Rust context rejected? Does each error give a usable repair before server work? | schema/input; wrong type/field/combination |
| LSP-12 | After changing only entry's direct import/call in a disposable copy, what reference/caller changes appear? Restore source and verify original facts. Retain diagnostics and first reacquisition after idle. | freshness; cache; lifecycle |
| LSP-13 | On an installed Rust server and frozen minimal Rust source, can definition/references/hover/documentSymbols resolve a local function? Which project/build settings constrain coverage? | Rust smoke; explicitly unverified if unavailable |

The deciding rival is a direct client of the exact same installed language server and compiler.
Separate initialize/didOpen/readiness from first and warm requests. Source synchronization is wire
traffic, not model context. Compare locations with a shared projection; report raw wire output and
Octocode source/role/diagnostic enrichment separately. `rg -w` is only a spelling baseline: report
its false matches and missing aliases, never present it as semantic authority.
