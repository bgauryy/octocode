# Octocode Search Parity Fixes — Implementation Plan

**Status:** planning doc for three findings surfaced by the
[OCTOCODE_SEARCH_PARITY_CHECKLIST](https://github.com/bgauryy/octocode/blob/main/docs/octocode-language/OCTOCODE_SEARCH_PARITY_CHECKLIST.md)
audit on 2026-06-22. F1 and the TDD test scaffolding are gate-free and ready to
implement. **F2 is at a hard gate awaiting a decision** on the classifier's
home module (§Decision Summary). F3 requires no code change.

Authoritative contract:
[OCTOCODE_QUERY_LANGUAGE](https://github.com/bgauryy/octocode/blob/main/docs/octocode-language/OCTOCODE_QUERY_LANGUAGE.md).
Parent plan:
[OCTOCODE_QUERY_LANGUAGE_PLAN](https://github.com/bgauryy/octocode/blob/main/docs/octocode-language/OCTOCODE_QUERY_LANGUAGE_PLAN.md).

## Scope (one line)

Fix three audit findings in `packages/octocode-tools-core/src/oql/` so that
`octocode search --scheme` and `octocode search --explain --dry-run` tell agents
the truth about `target:"diff"`, and so the `fetch.content` scheme text names
the field an agent must actually use.

All evidence below is from the rebuilt CLI on 2026-06-22 (`yarn build` clean;
`yarn vitest run` → 977 passing). GitHub auth via gh-cli was active during the
live probes.

## Decision Summary

Only F2 has a decision; F1 and F3 are settled.

| Fix | Verdict | Gate | Reason |
|---|---|---|---|
| **F1** scheme text names `contentView` | Implement now | none | One string literal; no type/code semantics touched. |
| **F2** dry-run truthful for `target:"diff"` | **Awaiting decision** (3 options below) | hard (smallest-vs-safest) | Root cause is structural: lane discriminant lives only in the executor. |
| **F3** continuation dotted keys | No code change | none | Confirmed intentional contract; renaming would break documented surface. |

### F2 options (the gate)

The diff-lane discriminant currently lives only in `executeDiff`
(`adapters/v2.ts:284-318`): `hasPr = p.prNumber != null` and
`directFileRefs(p)` (`v2.ts:327-341`). The planner has no access to it, so
`backendForTargetless('diff')` (`planner.ts:300`) returns `'ghHistoryResearch'`
unconditionally — wrong for 2 of 3 lanes.

| Option | Home for `classifyDiffLane(params)` | Rating | Tradeoff |
|---|---|---:|---|
| **A — new pure module `oql/diffLanes.ts`** | own file, imported by `planner.ts` + `adapters/v2.ts` | **9/10** | Dependency rule clean (both depend on a stable pure helper; no planner↔adapter edge). SRP precise. Costs one ~20-line file. |
| B — inline in `backendForTargetless` (`planner.ts:300`) | planner body | 5/10 | Smallest diff (~10 LOC). **Duplicates** `directFileRefs` logic → drift risk when a 3rd lane is added. |
| C — export from `capabilities.ts` | `capabilities.ts` | 3/10 | Layer mismatch: `capabilities.ts` is predicate-routing (PUSHDOWN/RESIDUAL/ROUTE over `CapabilityContext` + leaf predicate, `capabilities.ts:1-30`); it does not consume `params`. Wrong concern. |

> **Correction note.** An earlier version of this plan recommended exporting the
> classifier from `capabilities.ts`. That recommendation was retracted after
> reading `capabilities.ts:1-60`: its public surface (`CapabilityContext`,
> `CapabilityDecision`, `routeLeafPredicate`) is exclusively about leaf-predicate
> routing and does not take `params`. Adding a params-shape classifier there
> would violate Single Responsibility (analytic dimension #5).

**Recommendation: Option A.** It is the only option that (a) removes the
duplication root cause (one source of truth for the lane discriminant) and
(b) preserves the dependency rule (planner and adapter both depend *downward*
on a pure helper, never on each other). Option B's "smallest diff" appeal is
exactly the trap the skill's smallest-vs-safest gate exists to catch: a
narrow patch over a structural cause.

**Blocking:** F2 implementation will not start until you pick A / B / C. F1 and
the TDD test scaffolding (§Test Plan) may proceed independently — say so if you
want that.

## F1 — scheme text names `contentView`

**Root cause.** `schemeText.ts:30` renders the `fetch` hint as the literal
string `'{ content?: {...}, tree?: {...} }'`. An agent reading only `--scheme`
cannot discover that the real field is `contentView` (not `view`, which is
rejected at normalize with `fetch.content: Unrecognized key: "view"`). The
correct name is defined at:

- `features.ts:38` — `query.fetch?.content?.contentView` (drives the
  `signatureUnsupported` diagnostic on `symbols` for PR/commit/diff)
- `defaults.ts:41-42` — `applied['fetch.content.contentView']` and
  `DEFAULTS.contentView`
- `run.ts:143,155` — continuation builders write `contentView: 'exact'`

**File.** `packages/octocode-tools-core/src/oql/schemeText.ts:30`

**Change** (single string replacement):

```ts
// before
fetch: '{ content?: {...}, tree?: {...} }',
// after
fetch:
  '{ content?: { contentView:"exact"|"compact"|"symbols", range?:{startLine?,endLine?,contextLines?}, charOffset?, charLength? }, tree?: {...} }',
```

The values come from `features.ts:38` (`exact`/`compact`/`symbols`) and
`defaults.ts:42` (`contentCharLength` ⇒ `charLength`). No type or code
semantics change; `OQL_SCHEMA_DOC` is `as const` string data.

**Verification.** `node packages/octocode/out/octocode.js search --scheme` →
the `query.fetch` line must contain `contentView`. No test needed (string in a
doc blob); covered by a manual eyeball + the F2 test's `--scheme` assertion.

## F2 — dry-run truthful for `target:"diff"`

### Current (broken) flow

`search --explain --dry-run` calls `planQuery` (`planner.ts:185`). For
`target:"diff"` there is no `where` (V2 targetless), so the `else` branch at
`planner.ts:213` fires:

```ts
out.backendCalls.push({
  backend: backendForTargetless(query),   // planner.ts:280
  source,
  operation: operationFor(query.target),  // 'diff' for target:'diff'
  exact: true,
});
```

`backendForTargetless` switches on `query.target` (`planner.ts:280-305`) and
returns `'ghHistoryResearch'` for `case 'diff'` (`planner.ts:300`) —
**ignoring `query.params` entirely**.

At execution, `run.ts` dispatches via the adapter registry to
`executeDiff(query)` (`adapters/v2.ts:280`), which branches on `params`:

| `params` shape | Execution backend(s) | Execution `diagnostics` |
|---|---|---|
| `{prNumber, files?}` | `ghHistoryResearch` (PR patches) | `[]` |
| `{baseRef, headRef, path}` | 2 × `ghGetFileContent` + local `computeLineDiff` | `[]` (or `zeroMatches` info if identical) |
| neither | (no call) | `invalidQuery` + repair, `blocksAnswer:true` |

So for 2 of 3 lanes, the dry-run plan contradicts execution on **both** the
backend name and the `executable` flag.

### Fix (Option A — recommended)

**New file.** `packages/octocode-tools-core/src/oql/diffLanes.ts`

Pure, no imports beyond types:

```ts
import type { OqlQueryV1 } from './types.js';

export type DiffLane =
  | { kind: 'prPatch'; prNumber: number; files?: string[] }
  | { kind: 'directFile'; baseRef: string; headRef: string; path: string }
  | { kind: 'neither' };

/** Unified lane discriminant for target:"diff". Single source of truth. */
export function classifyDiffLane(
  params: Record<string, unknown> | undefined
): DiffLane {
  const p = params ?? {};
  if (p.prNumber !== undefined && p.prNumber !== null) {
    return {
      kind: 'prPatch',
      prNumber: p.prNumber as number,
      ...(Array.isArray(p.files) ? { files: p.files as string[] } : {}),
    };
  }
  if (
    typeof p.baseRef === 'string' &&
    typeof p.headRef === 'string' &&
    typeof p.path === 'string'
  ) {
    return { kind: 'directFile', baseRef: p.baseRef, headRef: p.headRef, path: p.path };
  }
  return { kind: 'neither' };
}

/** Backend name the planner should announce for a lane. */
export function diffLaneBackend(lane: DiffLane): string {
  switch (lane.kind) {
    case 'prPatch': return 'ghHistoryResearch';
    case 'directFile': return 'ghGetFileContent';
    case 'neither': return ''; // no call planned
  }
}
```

**Edits.**

1. `packages/octocode-tools-core/src/oql/planner.ts` — add import; replace the
   unconditional `case 'diff'` inside `backendForTargetless` (line ~300) with a
   branch that uses the lane. Because `backendForTargetless` currently returns
   only a string, the diff path needs to also push a diagnostic for the
   neither-lane. Cleanest: special-case diff *before* the generic targetless
   push at `planner.ts:213-219`, e.g.:

   ```ts
   // inside planQuery, replacing the generic else-branch for diff only
   if (query.target === 'diff') {
     const lane = classifyDiffLane(query.params as Record<string, unknown> | undefined);
     const backend = diffLaneBackend(lane);
     if (backend) {
       out.backendCalls.push({
         backend,
         source,
         operation: operationFor('diff'),
         exact: true,
       });
     } else {
       out.diagnostics.push(
         diagnostic('invalidQuery',
           'target:"diff" needs either {prNumber} (PR patch diff) or {baseRef,headRef,path} (direct file diff between two refs).',
           { queryPath: 'params', backend: 'ghHistoryResearch',
             repair: { message: 'Add params.prNumber for a PR patch, or params.baseRef + params.headRef + params.path for a direct file diff.' } })
       );
     }
   } else {
     // existing generic targetless branch (planner.ts:213-219)
     out.backendCalls.push({ backend: backendForTargetless(query), source, operation: operationFor(query.target), exact: true });
   }
   ```

   Leave `backendForTargetless`'s `case 'diff'` as a defensive fallback (it
   will no longer be reached for diff) or delete it — pick one, do not leave
   dead ambiguity. Preference: delete, since the dispatch above is now the
   single owner (repo rule: "One owner per behavior").

2. `packages/octocode-tools-core/src/oql/adapters/v2.ts:280-318` — refactor
   `executeDiff` to consume `classifyDiffLane` instead of its private
   `hasPr` / `directFileRefs`. The `directFileRefs` helper at `v2.ts:327-341`
   and the `DirectFileRefs` interface at `v2.ts:325-330` become dead — delete
   them (do not leave a second implementation; repo's "no backward compat by
   default" rule).

   The execution error paths stay: the "Direct file diff needs a concrete
   owner/repo" guard (`v2.ts:354-360`) and the content-read-failure guard
   (`v2.ts:385-392`) remain inside `executeDirectFileDiff`; they are runtime
   checks the planner cannot know about and correctly stay in the adapter.

### Behavior after fix (plan == execution)

| diff `params` | dry-run `backendCalls` | dry-run `diagnostics` | `executable` |
|---|---|---|---|
| `{prNumber}` | `[{ghHistoryResearch, diff, exact}]` | `[]` | `true` |
| `{baseRef,headRef,path}` | `[{ghGetFileContent, diff, exact}]` | `[]` | `true` |
| neither | `[]` | `[{invalidQuery + repair, blocksAnswer:true}]` | `false` |

The `executable` flag flips correctly because `planQuery` derives it at
`planner.ts:285` as `!diagnostics.some(d => d.severity==='error' && d.code!=='planTruncated')`,
and `invalidQuery` is in `ERROR_CODES` (`diagnostics.ts:46`).

### Out-of-scope follow-up (flagged, not fixed)

The direct-file lane makes **two** `ghGetFileContent` reads
(`v2.ts:377-378`, `Promise.all([read(baseRef), read(headRef)])`). The plan
announces one logical call. Honoring that would require `OqlBackendCall` to
carry ref multiplicity — a contract change outside this fix's scope. Note in
the checklist's gap log when this fix lands.

## F3 — continuation dotted keys

**Verdict: no code change.** Confirmed intentional, not a bug.

**Evidence.** `run.ts:108-227` builds the envelope `next` and per-row `next`
as a flat `Record<string, OqlContinuation>` with dotted domain-name keys:

```ts
next['next.page']      = { query, why, confidence }            // run.ts:111
out['next.fetch']      = { query, why, confidence }            // run.ts:133
out['next.semantic']   = { query, why, confidence }            // run.ts:143
out['next.charRange']  = { query, why, confidence }            // buildContentContinuations
```

The `attachContinuations` jsdoc (`run.ts:108-118`) documents this as the
contract: "Per-row continuations are produced by a registry keyed by row kind
… code → next.fetch (read exact content) [+ next.semantic on local]; content →
next.charRange; artifact → next.structure / next.files; …". The checklist
itself documents `next.fetch`, `next.artifactStrings`, `next.page`,
`next.structure`, `next.files`, `next.charRange` as the surface (Gap 4, Gap 6,
Gap 9). Renaming would break the documented contract, every builder in
`ROW_CONTINUATION_BUILDERS` (`run.ts:125-131`), and any external consumer.

**Optional doc polish (gate-free).** Add a one-line note at the
`OqlResultRow.next` declaration in `packages/octocode-tools-core/src/oql/types.ts`
clarifying: "Keys are dotted domain names (`next.<domain>`, e.g.
`next.fetch`, `next.semantic`), not nested object paths." This makes the
convention explicit at the type rather than only in `run.ts` jsdoc. Pure
comment, no behavior change.

## Test Plan (TDD)

Per repo methodology (AGENTS.md: "Plan → TDD (failing test → `yarn test` →
fix) → `yarn lint` → verify"). The parity gap exists because
`packages/octocode-tools-core/tests/oql/v2-targets.test.ts` covers the PR-patch
lane (line ~62: `{target:'diff', repo:'facebook/react', params:{prNumber:1}} →
ghHistoryResearch/diff`) but has **no** direct-file or neither-shape case.

**File.** Extend `packages/octocode-tools-core/tests/oql/v2-targets.test.ts`
(matching existing convention: `plan()` helper at line 7–10 wraps
`normalizeQuery` + `planQuery`).

**New cases** (write first, watch fail against current code, then implement F2):

```ts
// direct-file lane — currently fails: planner returns 'ghHistoryResearch'
it('diff direct-file lane routes to ghGetFileContent', () => {
  const { plan: p, executable } = plan({
    target: 'diff',
    repo: 'facebook/react',
    params: { baseRef: 'main', headRef: 'feature', path: 'README.md' },
  });
  expect(executable).toBe(true);
  expect(p.backendCalls).toHaveLength(1);
  expect(p.backendCalls[0]?.backend).toBe('ghGetFileContent');
  expect(p.backendCalls[0]?.operation).toBe('diff');
});

// neither shape — currently fails: planner says executable + 1 ghHistoryResearch call
it('diff with neither lane shape is not executable and emits invalidQuery repair', () => {
  const { plan: p, executable } = plan({
    target: 'diff',
    repo: 'facebook/react',
  });
  expect(executable).toBe(false);
  expect(p.backendCalls).toHaveLength(0);
  const diag = p.diagnostics.find(d => d.code === 'invalidQuery');
  expect(diag).toBeDefined();
  expect(diag?.blocksAnswer).toBe(true);
  expect(diag?.repair?.message).toMatch(/prNumber/);
  expect(diag?.repair?.message).toMatch(/baseRef/);
});
```

The existing PR-patch case stays green throughout (regression guard).

**Optional F2 unit test for the classifier** (new file
`tests/oql/diffLanes.test.ts`): direct unit tests on `classifyDiffLane` /
`diffLaneBackend` for all three lanes + edge cases (`prNumber:0` is valid;
`prNumber:null` falls through to direct-file/neither). Cheap, fast, pins the
discriminant independently of the planner.

## Verification

```bash
yarn build                                                 # after any src edit
yarn workspace @octocodeai/octocode-tools-core test tests/oql/v2-targets.test.ts
yarn workspace @octocodeai/octocode-tools-core test tests/oql/diffLanes.test.ts   # if added
yarn workspace @octocodeai/octocode-tools-core test                                    # full suite, expect still 977+
yarn lint
yarn typecheck

# Live dry-run spot-checks (no network — planQuery is pure):
node packages/octocode/out/octocode.js search --explain --dry-run --json \
  --query '{"target":"diff","from":{"kind":"github","repo":"octocode/octocode"},"params":{"prNumber":123}}'
# → backendCalls[0].backend == "ghHistoryResearch"

node packages/octocode/out/octocode.js search --explain --dry-run --json \
  --query '{"target":"diff","from":{"kind":"github","repo":"octocode/octocode"},"params":{"baseRef":"main","headRef":"feature","path":"README.md"}}'
# → backendCalls[0].backend == "ghGetFileContent"

node packages/octocode/out/octocode.js search --explain --dry-run --json \
  --query '{"target":"diff","from":{"kind":"github","repo":"octocode/octocode"}}'
# → backendCalls == []; diagnostics[0].code == "invalidQuery"; executable == false

# F1 spot-check:
node packages/octocode/out/octocode.js search --scheme | grep contentView
# → the query.fetch line contains "contentView"
```

## Compatibility & Rollout

- **F1**: string-only change to a doc blob. No behavioral surface affected.
- **F2**: `OqlExplainPlan` *shape* unchanged (`backendCalls[]`, `diagnostics[]`,
  `executable`). Values become more accurate. Treat as a **bugfix**, not a
  breaking contract change. Agents that happened to key off "diff always shows
  `ghHistoryResearch`" will see more-accurate names; no migration needed.
- **F3**: no change.
- **No public API, schema, wire protocol, DB, or CLI flag touched.** No
  migration, no feature flag, no rollback path beyond `git revert`.

## Architecture Health (per the engineer lens)

| Principle / Dimension | Verdict | Source |
|---|---|---|
| Dependency rule | `confirmed` ok under Option A | new `diffLanes.ts` is a leaf; planner + adapter both depend downward |
| Layer boundaries | `confirmed` ok | fix stays in `packages/octocode-tools-core/src/oql/` |
| Boundary ownership | `confirmed` ok | one classifier, one owner; `directFileRefs` deleted from `v2.ts` |
| Single responsibility | `confirmed` ok | `diffLanes.ts` has one reason to change (a new diff lane) |
| 1 Flows | `confirmed` traced end-to-end | `planner.ts:213` → `v2.ts:280` |
| 2 Duplication | `confirmed` resolved by Option A | `directFileRefs` (`v2.ts:327`) removed; classifier is the single source |
| 3 Types | `confirmed` ok | `OqlBackendCall` / `OqlDiagnostic` reused; new `DiffLane` is a tagged union |
| 4 Protocols/schemas | `confirmed` ok | `--explain` output shape unchanged |
| 5 Data flows | N/A | planner is pure, no state |
| 6 Execution | N/A | no runtime/async change |

## Open Question (the gate)

**F2 classifier home: A / B / C?** Recommendation: **A** (new `diffLanes.ts`).
Until you confirm, F2 implementation is blocked. F1 + the TDD test scaffolding
can proceed in parallel if you want — say so.
