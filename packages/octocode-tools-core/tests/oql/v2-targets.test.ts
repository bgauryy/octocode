import { describe, expect, it } from 'vitest';
import { normalizeQuery } from '../../src/oql/normalize.js';
import { planQuery } from '../../src/oql/planner.js';
import type { OqlQuery } from '../../src/oql/types.js';

function plan(input: unknown) {
  const q = normalizeQuery(input as never) as OqlQuery;
  return planQuery(q, input);
}

describe('OQL research targets are active and route to their backend', () => {
  const cases: Array<{ input: unknown; backend: string; op: string }> = [
    {
      input: { target: 'repositories', params: { keywords: ['oql'] } },
      backend: 'ghSearchRepos',
      op: 'searchRepos',
    },
    {
      input: { target: 'packages', params: { packageName: 'react' } },
      backend: 'npmSearch',
      op: 'searchPackages',
    },
    {
      input: {
        target: 'pullRequests',
        repo: 'facebook/react',
        params: { state: 'merged' },
      },
      backend: 'ghHistoryResearch',
      op: 'searchPullRequests',
    },
    {
      input: {
        target: 'commits',
        repo: 'facebook/react',
        params: { path: 'packages/react' },
      },
      backend: 'ghHistoryResearch',
      op: 'searchCommits',
    },
    {
      input: {
        target: 'diff',
        repo: 'facebook/react',
        params: { prNumber: 1 },
      },
      backend: 'ghHistoryResearch',
      op: 'diff',
    },
    {
      input: {
        target: 'semantics',
        from: { kind: 'local', path: './a.ts' },
        params: { type: 'definition', symbolName: 'x', lineHint: 1 },
      },
      backend: 'lspGetSemantics',
      op: 'getSemantics',
    },
    {
      input: {
        target: 'research',
        from: { kind: 'local', path: './' },
        params: { goal: 'find unused exports and dependencies' },
      },
      backend: 'smartOqlResearch',
      op: 'runResearchFlow',
    },
  ];

  for (const c of cases) {
    it(`${(c.input as { target: string }).target} -> ${c.backend}`, () => {
      const { plan: p, executable } = plan(c.input);
      expect(executable).toBe(true);
      expect(p.backendCalls[0]?.backend).toBe(c.backend);
      expect(p.backendCalls[0]?.operation).toBe(c.op);
    });
  }

  it('does not treat artifacts as an active target', () => {
    expect(() =>
      plan({
        target: 'artifacts',
        from: { kind: 'local', path: './a.node' },
        params: { mode: 'inspect' },
      })
    ).toThrow();
  });

  it('local structural code search reports the structural transformer', () => {
    const { plan: p, executable } = plan({
      target: 'code',
      from: { kind: 'local', path: './src' },
      where: {
        kind: 'structural',
        lang: 'ts',
        pattern: 'function $NAME($$$ARGS) { $$$BODY }',
      },
    });
    expect(executable).toBe(true);
    expect(p.transformers?.map(t => t.id)).toContain('local.code.structural');
    expect(p.transformers?.map(t => t.id)).not.toContain(
      'local.code.textRegex'
    );
  });

  it('package target explain uses npmSearch provenance', () => {
    const { plan: p, executable } = plan({
      target: 'packages',
      params: { packageName: 'zod' },
    });
    expect(executable).toBe(true);
    expect(p.transformers?.map(t => t.id)).toEqual(['npm.packages']);
    expect(p.backendCalls[0]).toMatchObject({
      backend: 'npmSearch',
      operation: 'searchPackages',
    });
  });

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

  it('local diff direct-file lane reports local diff transformer provenance', () => {
    const { plan: p, executable } = plan({
      target: 'diff',
      from: { kind: 'local', path: './a.ts' },
      params: { baseRef: 'left', headRef: 'right', path: './b.ts' },
    });
    expect(executable).toBe(true);
    expect(p.transformers?.map(t => t.id)).toEqual(['local.diff.directFile']);
    expect(p.backendCalls[0]).toMatchObject({
      backend: 'localGetFileContent',
      operation: 'diff',
    });
  });

  it('remote semantics plans its materialize then LSP adapter chain', () => {
    const { plan: p, executable } = plan({
      target: 'semantics',
      repo: 'facebook/react',
      params: { type: 'definition', uri: 'packages/react/index.js' },
    });
    expect(executable).toBe(true);
    expect(p.transformers?.[0]?.id).toBe('github.semantics');
    expect(p.backendCalls.map(c => c.backend)).toEqual([
      'ghCloneRepo',
      'lspGetSemantics',
    ]);
    expect(p.normalized.materialize).toMatchObject({
      mode: 'required',
      strategy: 'file',
    });
    expect(p.materialization).toMatchObject({
      mode: 'required',
      strategy: 'file',
      required: true,
    });
  });

  it('GitHub files with no predicate plans materialize then local file listing when allowed', () => {
    const { plan: p, executable } = plan({
      target: 'files',
      repo: 'facebook/react',
      path: 'packages/react',
      materialize: { mode: 'auto', strategy: 'subtree' },
    });
    expect(executable).toBe(true);
    expect(p.transformers?.map(t => t.id)).toEqual([
      'github.materialize',
      'local.files',
    ]);
    expect(p.backendCalls.map(c => c.backend)).toEqual([
      'ghCloneRepo',
      'localFindFiles',
    ]);
  });

  it('GitHub files with a routed predicate explains materialize then local file proof', () => {
    const { plan: p, executable } = plan({
      target: 'files',
      repo: 'facebook/react',
      path: 'packages/react',
      text: 'TODO',
      materialize: { mode: 'auto', strategy: 'subtree' },
    });
    expect(executable).toBe(true);
    expect(p.nodes[0]?.route).toBe('ROUTE');
    expect(p.transformers?.map(t => t.id)).toEqual([
      'github.materialize',
      'local.files',
    ]);
    expect(p.backendCalls.map(c => c.backend)).toEqual([
      'ghCloneRepo',
      'localFindFiles',
    ]);
  });

  it('GitHub files provider-only trace stays approximate when not materialized', () => {
    const { plan: p, executable } = plan({
      target: 'files',
      repo: 'facebook/react',
      text: 'TODO',
      materialize: 'never',
    });
    expect(executable).toBe(true);
    expect(p.backendCalls[0]).toMatchObject({
      backend: 'ghSearchCode',
      operation: 'findFiles',
      exact: false,
    });
    expect(p.transformers?.[0]?.backends[0]).toMatchObject({
      backend: 'ghSearchCode',
      operation: 'findFiles',
      exact: false,
    });
  });

  it('GitHub code with required local proof explains materialize then local search', () => {
    const { plan: p, executable } = plan({
      target: 'code',
      repo: 'facebook/react',
      path: 'packages/react',
      text: 'TODO',
      materialize: { mode: 'required', strategy: 'subtree' },
    });
    expect(executable).toBe(true);
    expect(p.nodes[0]?.route).toBe('ROUTE');
    expect(p.transformers?.map(t => t.id)).toEqual([
      'github.materialize',
      'local.code.textRegex',
    ]);
    expect(p.backendCalls.map(c => c.backend)).toEqual([
      'ghCloneRepo',
      'localSearchCode',
    ]);
  });

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

  it('packages defaults the corpus to npm; repositories to provider-wide github', () => {
    const pkg = normalizeQuery({
      target: 'packages',
      params: { packageName: 'x' },
    } as never);
    expect(pkg.from).toEqual({ kind: 'npm' });
    const repos = normalizeQuery({
      target: 'repositories',
      params: { keywords: ['x'] },
    } as never);
    expect(repos.from).toEqual({ kind: 'github' });
  });

  it('params bag survives normalization', () => {
    const q = normalizeQuery({
      target: 'pullRequests',
      repo: 'a/b',
      params: { state: 'merged', author: 'me' },
    } as never);
    expect(q.params).toEqual({ state: 'merged', author: 'me' });
  });

  it('reserved fixes/dataflow still unsupported', () => {
    for (const target of ['fixes', 'dataflow']) {
      expect(() =>
        normalizeQuery({ target, from: { kind: 'local', path: '.' } } as never)
      ).toThrow();
    }
  });
});
