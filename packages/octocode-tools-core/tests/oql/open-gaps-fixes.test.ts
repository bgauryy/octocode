/**
 * Regression tests for the OQL coverage-gap closures (OCTOCODE_SEARCH_PARITY_
 * CHECKLIST.md gap log #7–12,18). Pure helpers, planner diagnostics, and real
 * local execution —
 * no backend mocking. Execution paths that need a clone/inspect backend are
 * covered in open-gaps-materialize.test.ts.
 */
import { describe, expect, it, beforeAll } from 'vitest';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { _resetConfigCache } from '../../src/shared/config/resolverCache.js';
import { runOqlSearch } from '../../src/oql/run.js';
import { normalizeQuery } from '../../src/oql/normalize.js';
import { planQuery } from '../../src/oql/planner.js';
import { checkOutputFeatures } from '../../src/oql/features.js';
import { mapCodeResult } from '../../src/oql/adapters/resultMap.js';
import {
  computeLineDiff,
  executeDiff,
} from '../../src/oql/adapters/researchTargets.js';
import {
  isBatchEnvelope,
  type OqlCodeResultRow,
  type OqlQuery,
  type OqlResultEnvelope,
} from '../../src/oql/types.js';

const here = path.dirname(fileURLToPath(import.meta.url));
const OQL_SRC = path.resolve(here, '../../src/oql');

// The raw-tool path (executeDirectTool) gates local tools behind ENABLE_LOCAL;
// the direct-file diff lane reads files via that path, so enable it (and reset
// the cached config) — otherwise the read errors and a correct diff is impossible.
beforeAll(() => {
  process.env.ENABLE_LOCAL = 'true';
  _resetConfigCache();
});

function single(
  r: Awaited<ReturnType<typeof runOqlSearch>>
): OqlResultEnvelope {
  if (isBatchEnvelope(r)) throw new Error('expected single envelope');
  return r;
}
function plan(input: unknown) {
  const q = normalizeQuery(input as never) as OqlQuery;
  return planQuery(q, input);
}

/* ----------------------- gap 12a: metavars forwarding ------------------- */

describe('gap 12a: mapCodeResult forwards engine captures into row.metavars', () => {
  it('forwards metavars when the match carries them', () => {
    const result = {
      files: [
        {
          path: 'a.ts',
          matches: [{ line: 3, value: 'foo(x)', metavars: { X: 'x' } }],
        },
      ],
    };
    const mapped = mapCodeResult(result as never, { kind: 'local', path: '.' });
    const row = mapped.results[0] as OqlCodeResultRow;
    expect(row.metavars).toEqual({ X: 'x' });
  });

  it('omits metavars (never fabricates) when the match has none', () => {
    const result = { files: [{ path: 'a.ts', matches: [{ line: 3 }] }] };
    const mapped = mapCodeResult(result as never, { kind: 'local', path: '.' });
    const row = mapped.results[0] as OqlCodeResultRow;
    expect('metavars' in row).toBe(false);
  });

  it('preserves count-mode file totals when no match rows are returned', () => {
    const result = {
      files: [
        { path: 'a.ts', totalMatchedLines: 3 },
        { path: 'b.ts', totalOccurrences: 8 },
      ],
    };
    const mapped = mapCodeResult(result as never, { kind: 'local', path: '.' });
    expect(mapped.results[0]).toMatchObject({
      kind: 'code',
      path: 'a.ts',
      totalMatchedLines: 3,
    });
    expect(mapped.results[1]).toMatchObject({
      kind: 'code',
      path: 'b.ts',
      totalOccurrences: 8,
    });
  });
});

/* --------------- gap 11 + 12b: feature-capability diagnostics ----------- */

describe('gap 11: symbols content view on PR/commit/diff -> signatureUnsupported', () => {
  it('checkOutputFeatures flags symbols view on pullRequests', () => {
    const q = normalizeQuery({
      target: 'pullRequests',
      repo: 'facebook/react',
      fetch: { content: { contentView: 'symbols' } },
      params: { prNumber: 1 },
    } as never) as OqlQuery;
    const codes = checkOutputFeatures(q).map(d => d.code);
    expect(codes).toContain('signatureUnsupported');
  });

  it('does NOT flag symbols view on file content', () => {
    const q = normalizeQuery({
      target: 'content',
      from: { kind: 'local', path: './x.ts' },
      fetch: { content: { contentView: 'symbols' } },
    } as never) as OqlQuery;
    expect(checkOutputFeatures(q).map(d => d.code)).not.toContain(
      'signatureUnsupported'
    );
  });

  it('planner surfaces the diagnostic (non-blocking)', () => {
    const { plan: p } = plan({
      target: 'commits',
      repo: 'facebook/react',
      fetch: { content: { contentView: 'symbols' } },
      params: { path: 'src' },
    });
    const d = p.diagnostics.find(x => x.code === 'signatureUnsupported');
    expect(d).toBeDefined();
    expect(d?.blocksAnswer).toBe(false);
  });
});

describe('gap 12: structural metavar captures flow into rows', () => {
  it('forwards engine metavars (and ranges) end-to-end', async () => {
    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'local', path: OQL_SRC },
        where: {
          kind: 'structural',
          lang: 'ts',
          pattern: 'diagnostic($$$ARGS)',
        },
        select: ['metavars'],
        view: 'detailed',
      })
    );
    const withCaptures = env.results.find(
      r => r.kind === 'code' && (r as OqlCodeResultRow).metavars
    ) as OqlCodeResultRow | undefined;
    expect(withCaptures).toBeDefined();
    expect(withCaptures?.proofGrade).toBe('structural');
    expect(withCaptures?.metavars?.ARGS).toBeDefined();
    expect(Array.isArray(withCaptures?.metavars?.ARGS)).toBe(true);
  });

  it('does NOT emit a partialResult diagnostic (captures are available)', () => {
    const q = normalizeQuery({
      target: 'code',
      from: { kind: 'local', path: '.' },
      pattern: 'foo($$$ARGS)',
      lang: 'ts',
      select: ['metavars'],
    } as never) as OqlQuery;
    expect(checkOutputFeatures(q).map(d => d.code)).not.toContain(
      'partialResult'
    );
  });
});

/* ----------------------- gap 8: diff lane split ------------------------- */

describe('gap 8: computeLineDiff (pure)', () => {
  it('counts additions/deletions/unchanged', () => {
    const d = computeLineDiff('a\nb\nc', 'a\nB\nc\nd');
    expect(d.unchanged).toBe(2); // a, c
    expect(d.deletions).toBe(1); // b
    expect(d.additions).toBe(2); // B, d
    expect(d.patch).toContain('- b');
    expect(d.patch).toContain('+ B');
    expect(d.patch).toContain('+ d');
  });

  it('identical files -> zero changes', () => {
    const d = computeLineDiff('x\ny', 'x\ny');
    expect(d.additions).toBe(0);
    expect(d.deletions).toBe(0);
    expect(d.unchanged).toBe(2);
  });
});

describe('gap 8: diff with neither prNumber nor base/head refs -> repair', () => {
  it('returns invalidQuery repair instead of a silent PR call', async () => {
    const res = await executeDiff({
      schema: 'oql',
      target: 'diff',
      from: { kind: 'github', repo: 'facebook/react' },
      params: {},
    } as OqlQuery);
    expect(res.results).toHaveLength(0);
    const d = res.diagnostics[0];
    expect(d?.code).toBe('invalidQuery');
    expect(d?.repair?.message).toMatch(/prNumber|baseRef/);
  });

  it('executes local ref diff via git show (same ref -> identical)', async () => {
    // The lane contract is "path at baseRef vs path at headRef" — both sides
    // come from git object storage, so from.path is the repo/workdir anchor
    // and params.path is repo-relative. Same ref twice must report identical.
    const res = await executeDiff({
      schema: 'oql',
      target: 'diff',
      from: { kind: 'local', path: here },
      params: {
        baseRef: 'HEAD',
        headRef: 'HEAD',
        path: 'open-gaps-fixes.test.ts',
      },
    } as OqlQuery);
    expect(res.diagnostics.map(d => d.code)).not.toContain('invalidQuery');
    expect(res.provenance[0]).toMatchObject({ backend: 'git' });
    expect(res.results[0]).toMatchObject({
      kind: 'record',
      recordType: 'diff',
      id: 'HEAD..HEAD:open-gaps-fixes.test.ts',
      data: {
        path: 'open-gaps-fixes.test.ts',
        baseRef: 'HEAD',
        headRef: 'HEAD',
        additions: 0,
        deletions: 0,
      },
    });
    expect(res.diagnostics.some(d => d.code === 'zeroMatches')).toBe(true);
  });

  it('rejects option-shaped git refs instead of passing them to git', async () => {
    const res = await executeDiff({
      schema: 'oql',
      target: 'diff',
      from: { kind: 'local', path: here },
      params: {
        baseRef: '--help',
        headRef: 'HEAD',
        path: 'open-gaps-fixes.test.ts',
      },
    } as OqlQuery);
    expect(res.results).toHaveLength(0);
    expect(res.diagnostics[0]?.code).toBe('invalidQuery');
    expect(res.diagnostics[0]?.message).toMatch(/plain git revisions/);
  });
});

/* ----------------- gap 7: materialize as addressable target ------------- */

describe('gap 7: target:"materialize" planning', () => {
  it('rejects a `where` predicate', () => {
    expect(() =>
      normalizeQuery({
        target: 'materialize',
        repo: 'facebook/react',
        text: 'foo',
      } as never)
    ).toThrow(/materialize/);
  });

  it('refuses an unbounded materialization (no scope.path)', () => {
    const { plan: p, executable } = plan({
      target: 'materialize',
      repo: 'facebook/react',
    });
    expect(executable).toBe(false);
    expect(p.diagnostics.map(d => d.code)).toContain(
      'materializationNotAllowed'
    );
  });

  it('plans a bounded clone checkpoint with scope.path', () => {
    const { plan: p, executable } = plan({
      target: 'materialize',
      repo: 'facebook/react',
      path: 'packages/react',
    });
    expect(executable).toBe(true);
    expect(p.backendCalls.map(b => b.backend)).toContain('ghCloneRepo');
  });
});

/* -------------------- #3: typed target params validation -------------------- */

describe('#3 typed target params: type mistakes -> invalidQuery', () => {
  it('rejects a wrongly-typed prNumber on diff', () => {
    expect(() =>
      normalizeQuery({
        target: 'diff',
        repo: 'facebook/react',
        params: { prNumber: 'not-a-number' },
      } as never)
    ).toThrow(/params\.prNumber/);
  });

  it('rejects a wrongly-typed page on repositories', () => {
    expect(() =>
      normalizeQuery({
        target: 'repositories',
        params: { keywords: ['x'], page: 'one' },
      } as never)
    ).toThrow(/params\.page/);
  });

  it('accepts valid params + passes through unknown fields', () => {
    const q = normalizeQuery({
      target: 'pullRequests',
      repo: 'facebook/react',
      params: { prNumber: 5, state: 'merged', someFutureField: true },
    } as never) as OqlQuery;
    expect(q.params?.prNumber).toBe(5);
    expect(q.params?.someFutureField).toBe(true);
  });

  it('rejects an invalid semantics type enum', () => {
    expect(() =>
      normalizeQuery({
        target: 'semantics',
        from: { kind: 'local', path: './x.ts' },
        params: { type: 'not-a-real-op' },
      } as never)
    ).toThrow(/params\.type/);
  });

  it('accepts every semantics type exposed by lspGetSemantics', () => {
    for (const type of [
      'definition',
      'references',
      'callers',
      'callees',
      'callHierarchy',
      'hover',
      'documentSymbols',
      'typeDefinition',
      'implementation',
      'workspaceSymbol',
      'supertypes',
      'subtypes',
      'diagnostic',
    ]) {
      expect(() =>
        normalizeQuery({
          target: 'semantics',
          from: { kind: 'local', path: './x.ts' },
          params: { type },
        } as never)
      ).not.toThrow();
    }
  });
});

/* ---------------- gap 10: code rows emit next.semantic ------------------ */

describe('gap 10: local code rows carry next.fetch + next.semantic', () => {
  it('emits both continuations on a local code hit', async () => {
    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'local', path: OQL_SRC },
        where: { kind: 'text', value: 'runOqlSearch' },
        view: 'paginated',
      })
    );
    const code = env.results.find(r => r.kind === 'code') as OqlCodeResultRow;
    expect(code.next?.['next.fetch']).toBeDefined();
    expect(code.next?.['next.semantic']).toBeDefined();
    expect(code.next?.['next.semantic']?.query).toMatchObject({
      target: 'semantics',
    });
  });
});
