Load when classifying code as dead, duplicate, kludge, bloat, or tangled structure. Signal → removal bar. Defects to report instead of removing: [agent blockers](agent-blockers.md); agent leftovers and reinvention: [agent defects](agent-defects.md); suppressed checker findings: [checker suppressions](checker-suppressions.md); redundant declarations and dependencies: [types and dependencies](types-dependencies.md).

# Code smells

## Dead, duplicate, kludge

| Signal | Check / action |
|---|---|
| Logic-free re-export `export { X } from './X'`, barrel `export * from './m'`, alias `export default aliasedName` | references, import paths, package exports, public entrypoints traced; export behavior unchanged; graph misses external consumers. |
| Legacy shim (`legacy-marker` rows), old-API adapter, `if (version < X)` | no caller or live config needs the old path. |
| Duplicate bodies, copy-pasted constants, parallel helpers | AST match and diff, literal search, or graph confirms every site; pick canonical. |
| Module-scope `str.replace(/old-value/, ...)` fixup | fix the value at source. |
| `Object.assign(prototype, ...)` outside tests | patched object internal, owned. |
| Env check always true when deployed | confirmed on every target. |
| Repeated string literal | same meaning everywhere; extracting a constant is a consolidation batch, done only when the request covers consolidation. |
| One rule spelled out in many files that always change together (shotgun surgery) | co-change history (`graphQuery` operation `cochange`, or `git log` on the files) plus an AST or literal match of the copies; consolidate under one owner in its own batch. |
| One concept under several names in a change (`user`, `account`, `member`) | types or data flow prove one referent; keep the project's existing name. |
| Rust stringly identity (tool, vendor, error code as `String`/`&str` compared by literal) | producers, consumers traced; enum or newtype at the boundary. |
| Rust `Result<_, String>` beside a typed error | caller only formats; typed error keeps the variant. |

## Speculative generality and leftovers

| Signal | Check / action |
|---|---|
| One-implementation interface/ABC/Protocol, one-entry factory/registry/strategy table | no other implementor or registrant via public exports, plugins, config, test substitutes. |
| Option every caller passes identically, unused internal parameter, one-instantiation generic, forwarding-only wrapper | not exported; `lspSearch` lists all callers; no public contract, interface, or callback slot needs it. |
| Unimported generated type or module (often under module-wide `allow(dead_code)`) | check supported consumers and configurations, then trace output → generation target → producer and its other outputs. Remove only the obsolete target when the producer still serves live outputs; remove the producer only after all its outputs and consumers are proven unnecessary. Regenerate, check affected configurations, and remove an allowance only when no surviving item needs it. |
| Type-hint, decorator, annotation-only hunks on untouched functions | no runtime reader (pydantic, dataclasses, FastAPI, attrs, DI, reflection); else report. |
| Unreferenced `scratch-file` rows | no script, CI job, or doc runs them. |
| `ai-attribution` rows outside data, fixtures, transcripts | no disclosure or license policy requires them. Commit trailers and PR disclosure are not code comments: keep them. |

## Structure

| Signal | Check / action |
|---|---|
| Mixed responsibilities (IO, parsing, formatting, domain policy) | Group cohesive behavior at its owning layer; split only when consumers benefit. Update callers and verify. Size is no signal; large pure data is fine. |
| Prefixed flat names (`user-api.ts`, `user-db.ts`) | group only when shared ownership or navigation improves; naming alone does not justify a move. |
| One document | split only when readers load distinct parts for different tasks; otherwise keep the single entry. |
| Wrong layer (`utils/auth-logic.ts`, `api/db-queries.ts`) | Move to the owning domain or repository layer, update imports, verify with build and LSP; rename only a wrong name. |

## Spaghetti

Next step depends on flags, nested special cases, or cross-phase jumps; a long straight procedure is not spaghetti. High confidence only after reading the body, `astSearch` on its branch, loop, try shape, and `lspSearch` callers for passed flags; length, one `if`, or size proves nothing.

- Branch proven unreachable by callers, entrypoints, config → remove; move a phase without adding a branch.
- Else report class `spaghetti` (not safe to delete) with file, line range, signals: inner nested branch decides a different job (fused phases); boolean, mode, or enum set in one region, read in a distant branch; decision retested after a branch settled it; exception, labeled break, or callback jumping phases; cleanup typechecks only after a new flag, wrapper, condition, or copy (weaving).
