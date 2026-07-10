# Workflow: Refactor Mode

Use when reshaping structure, names, modules, or file layout while **preserving behavior and contracts**.
Hand off to `workflow-change.md` for new behavior / feature patches. Escalate contested architecture tradeoffs to `octocode-rfc-generator`. Read `algorithm.md` first; use `code-research.md` for the proof ladder.

```text
SKELETON → CONTRACTS → BLAST → PLAN (big→small) → EXECUTE → VERIFY → CLEAN
```

## Size tier (pick once)

| Tier | Scope | Default execute style |
|---|---|---|
| S | ≤3 files / one symbol | editor patches; LSP refs; targeted test |
| M | one package / module | task list; LSP rename + scoped patches; package verify |
| L | cross-package / directory tree | bulk `mv` first; mechanical path rewrites; LSP catch-up; layered verify |

Any size: orient cheap before deep reads. Scale the **plan and move tools**, not the proof grades.

## 1. Skeleton

Map shape before bodies:

1. `localViewStructure` (depth 1–2, then hotspots) + `localFindFiles` for entry/config globs.
2. `localGetFileContent` with `minify:"symbols"` on entry points and move targets.
3. `localSearchCode` `mode:"discovery"` / counts for rename/move anchors; `mode:"structural"` when shape (imports, exports, class/fn form) matters.
4. `lspGetSemantics` `documentSymbols` / `workspaceSymbol` for identity — never guess `lineHint`.

Stop when you can name: roots, public surface, move/rename set, and likely dependents.

## 2. Contracts (freeze before edits)

Inventory what must stay true:

- Public exports, types, schemas, CLI flags, MCP/tool names, env keys, serialized shapes.
- Tests, fixtures, and golden files as living contracts.
- Import/path conventions and package boundaries (`package.json` exports, tsconfig paths).

Ask before changing a public contract. Prefer adapter/shim only when the user explicitly wants compatibility; default is no silent API drift.
Record a one-line **invariant list** the refactor must not break.

## 3. Blast radius

For each symbol or path in scope:

- LSP `references` / `callers` (and `includeDeclaration:false` when counting consumers).
- Lexical/AST search across tests, scripts, configs, docs that LSP may miss.
- Diff lexical hits vs LSP before claiming “only / unused / safe.”

Gate: many consumers, cross-package edges, or deletes/renames of public names → confirm plan with the user.

## 4. Plan (big → small)

Break into ordered tasks with a verify gate each:

1. **Bulk layout** (L/M): create dirs, `mv` trees — real moves beat copy+delete.
2. **Mechanical path rewrites**: update import/require/path strings from the inventory (shell `sed`/`perl -pi` or equivalent **only** for proven path literals).
3. **Semantic renames**: LSP-anchored symbol renames / editor patches — never bulk-sed identifiers without identity proof.
4. **Internal cleanup inside moved units**: smallest scoped body edits; no opportunistic drive-bys.
5. **Contract check**: exports, types, and tests still match the invariant list.

Each task states: files touched, contract risk, verify command, rollback (reverse `mv` / restore from backup copy if needed).

## 5. Execute — tool choice

| Move | Prefer | Avoid |
|---|---|---|
| Directory / file relocate | `mkdir -p` + `mv` | rewrite-by-copy; one-file-at-a-time when a tree move works |
| Import/path string update after move | inventory → batched `sed`/`perl -pi` on the hit set | sed across the whole repo without a file list |
| Symbol rename | LSP refs + exact patches (or IDE rename) | blind sed on the identifier |
| Shape-preserving extract/inline | AST/structural locate → exact read → patch | rewrite from memory |
| Straggler hunt | `localSearchCode` + LSP `references` / `diagnostic` | assuming zero hits = safe |

Bulk-shell rules: operate on an explicit path list from search; dry-run or spot-check one file; re-run discovery after the batch; stop if hits remain outside the plan.

Shared-repo: declare presence / locks via `octocode-awareness` before wide moves.

## 6. Verify

Run what the tier demands; cite exit codes that **actually ran**:

- S: targeted unit/typecheck on touched symbols.
- M: package test + typecheck/lint.
- L: leaf packages → dependents → root build; plus LSP `diagnostic` on moved roots and a final lexical sweep for old paths/names.

Failed verify → read the failing path, patch only the cause, or report blocked. If evidence keeps shifting, escalate to `loop-mode.md`.

## 7. Clean / report

Remove dead paths only after delete-proof (`code-research.md` dead-code ladder). Do not mix unrelated cleanup.

Output: `Mode: Refactor` · tier · invariant list · task ledger · files moved/renamed · contracts preserved or explicitly changed · verification ran · confidence · next.

Validate: `node scripts/eval-research.mjs --case refactor-mode`.
