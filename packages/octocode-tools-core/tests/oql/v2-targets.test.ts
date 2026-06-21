import { describe, expect, it } from 'vitest';
import { normalizeQuery } from '../../src/oql/normalize.js';
import { planQuery } from '../../src/oql/planner.js';
import type { OqlQueryV1 } from '../../src/oql/types.js';

function plan(input: unknown) {
  const q = normalizeQuery(input as never) as OqlQueryV1;
  return planQuery(q, input);
}

describe('OQL V2 targets are active and route to their backend', () => {
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
        target: 'artifacts',
        from: { kind: 'local', path: './a.node' },
        params: { mode: 'inspect' },
      },
      backend: 'localBinaryInspect',
      op: 'inspectArtifact',
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
  ];

  for (const c of cases) {
    it(`${(c.input as { target: string }).target} -> ${c.backend}`, () => {
      const { plan: p, executable } = plan(c.input);
      expect(executable).toBe(true);
      expect(p.backendCalls[0]?.backend).toBe(c.backend);
      expect(p.backendCalls[0]?.operation).toBe(c.op);
    });
  }

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

  it('V3 fixes/dataflow still unsupported', () => {
    for (const target of ['fixes', 'dataflow']) {
      expect(() =>
        normalizeQuery({ target, from: { kind: 'local', path: '.' } } as never)
      ).toThrow();
    }
  });
});
