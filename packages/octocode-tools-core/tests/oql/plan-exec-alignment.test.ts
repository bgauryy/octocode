/**
 * Plan ↔ execution alignment: the planner's `executable` verdict and routing
 * must match what the adapters can actually do over a GitHub source. These were
 * previously inconsistent (planner said PUSHDOWN/executable while execution
 * returned requiresMaterialization / unsupportedBoolean).
 */
import { describe, expect, it } from 'vitest';
import { normalizeQuery } from '../../src/oql/normalize.js';
import { planQuery } from '../../src/oql/planner.js';
import type { OqlQuery } from '../../src/oql/types.js';
import { toGithubCodeSearchToolQuery } from '../../src/oql/transformers/github/code.js';

function plan(input: unknown) {
  const q = normalizeQuery(input as never) as OqlQuery;
  return planQuery(q, input);
}

describe('GitHub code multi-leaf boolean', () => {
  it('materialize:never -> NOT executable + requiresMaterialization', () => {
    const { plan: p, executable } = plan({
      target: 'code',
      from: { kind: 'github', repo: 'facebook/react' },
      scope: { path: 'packages/react' },
      where: {
        kind: 'all',
        of: [
          { kind: 'text', value: 'useEffect' },
          { kind: 'text', value: 'useState' },
        ],
      },
      materialize: 'never',
    });
    expect(executable).toBe(false);
    expect(p.diagnostics.some(d => d.code === 'requiresMaterialization')).toBe(
      true
    );
  });

  it('materialize:auto + scope.path -> executable via ROUTE (clone then local)', () => {
    const { plan: p, executable } = plan({
      target: 'code',
      from: { kind: 'github', repo: 'facebook/react' },
      scope: { path: 'packages/react' },
      where: {
        kind: 'all',
        of: [
          { kind: 'text', value: 'useEffect' },
          { kind: 'text', value: 'useState' },
        ],
      },
      materialize: { mode: 'auto', strategy: 'subtree' },
    });
    expect(executable).toBe(true);
    // The boolean node routes to materialization (clone -> local set-algebra).
    expect(p.nodes.some(n => n.route === 'ROUTE')).toBe(true);
  });

  it('boolean over github defaults to bounded materialization (auto)', () => {
    // No explicit materialize: a boolean composition is local-only over a
    // provider, so the normalizer defaults to materialize:auto.
    const q = normalizeQuery({
      target: 'code',
      from: { kind: 'github', repo: 'facebook/react' },
      scope: { path: 'packages/react' },
      where: {
        kind: 'any',
        of: [
          { kind: 'text', value: 'a' },
          { kind: 'text', value: 'b' },
        ],
      },
    } as never) as OqlQuery;
    expect(q.materialize?.mode).toBe('auto');
  });
});

describe('GitHub files lane', () => {
  it('files containing a term (text) + never -> executable provider path, approximate', () => {
    const { plan: p, executable } = plan({
      target: 'files',
      from: { kind: 'github', repo: 'facebook/react' },
      scope: { path: 'packages/react' },
      where: { kind: 'text', value: 'useEffect' },
      materialize: 'never',
    });
    expect(executable).toBe(true);
    const node = p.nodes.find(n => n.path === 'where');
    expect(node?.route).toBe('PUSHDOWN');
    expect(
      p.diagnostics.some(d => d.code === 'providerSemanticsApproximate')
    ).toBe(true);
  });

  it('files by path-field equality (extension) + never -> executable provider path pushdown', () => {
    const { plan: p, executable } = plan({
      target: 'files',
      from: { kind: 'github', repo: 'facebook/react' },
      where: { kind: 'field', field: 'extension', op: '=', value: 'ts' },
      materialize: 'never',
    });
    expect(executable).toBe(true);
    const node = p.nodes.find(n => n.path === 'where');
    expect(node?.route).toBe('PUSHDOWN');
    expect(node?.backend).toBe('ghSearchCode');
  });

  it('files by non-path field (size) + never -> NOT executable + requiresMaterialization', () => {
    const { plan: p, executable } = plan({
      target: 'files',
      from: { kind: 'github', repo: 'facebook/react' },
      where: { kind: 'field', field: 'size', op: '>', value: 100 },
      materialize: 'never',
    });
    expect(executable).toBe(false);
    expect(p.diagnostics.some(d => d.code === 'requiresMaterialization')).toBe(
      true
    );
  });

  it('files by field + auto -> ROUTE to materialization', () => {
    const { plan: p, executable } = plan({
      target: 'files',
      from: { kind: 'github', repo: 'facebook/react' },
      scope: { path: 'packages/react' },
      where: { kind: 'field', field: 'extension', op: '=', value: 'ts' },
      materialize: { mode: 'auto', strategy: 'subtree' },
    });
    expect(executable).toBe(true);
    expect(p.nodes.some(n => n.route === 'ROUTE')).toBe(true);
  });

  it('files with no where + never -> NOT executable + requiresMaterialization', () => {
    const { plan: p, executable } = plan({
      target: 'files',
      from: { kind: 'github', repo: 'facebook/react' },
      materialize: 'never',
    });
    expect(executable).toBe(false);
    expect(p.diagnostics.some(d => d.code === 'requiresMaterialization')).toBe(
      true
    );
  });
});

describe('GitHub code lane: case-sensitive / whole-word text is not proof (C1)', () => {
  it('plain literal text + never -> PUSHDOWN exact, no approximate diagnostic', () => {
    const { plan: p, executable } = plan({
      target: 'code',
      from: { kind: 'github', repo: 'facebook/react' },
      where: { kind: 'text', value: 'useEffect' },
      materialize: 'never',
    });
    expect(executable).toBe(true);
    const node = p.nodes.find(n => n.path === 'where');
    expect(node?.route).toBe('PUSHDOWN');
    expect(
      p.diagnostics.some(d => d.code === 'providerSemanticsApproximate')
    ).toBe(false);
  });

  it('case:sensitive text + never -> PUSHDOWN but approximate (provider cannot honor case)', () => {
    const { plan: p, executable } = plan({
      target: 'code',
      from: { kind: 'github', repo: 'facebook/react' },
      where: { kind: 'text', value: 'useEffect', case: 'sensitive' },
      materialize: 'never',
    });
    expect(executable).toBe(true);
    const node = p.nodes.find(n => n.path === 'where');
    expect(node?.route).toBe('PUSHDOWN');
    expect(
      p.diagnostics.some(d => d.code === 'providerSemanticsApproximate')
    ).toBe(true);
  });

  it('wholeWord text + auto + scope.path -> ROUTE to local for exact proof', () => {
    const { plan: p, executable } = plan({
      target: 'code',
      from: { kind: 'github', repo: 'facebook/react' },
      scope: { path: 'packages/react' },
      where: { kind: 'text', value: 'useEffect', wholeWord: true },
      materialize: { mode: 'auto', strategy: 'subtree' },
    });
    expect(executable).toBe(true);
    expect(p.nodes.some(n => n.route === 'ROUTE')).toBe(true);
  });
});

describe('plan/exec agreement per leaf shape (github, materialize:never)', () => {
  const CASES: Array<{ name: string; target: string; where: unknown }> = [
    {
      name: 'text content over code',
      target: 'code',
      where: { kind: 'text', value: 'useEffect' },
    },
    {
      name: 'rust-dialect regex over code',
      target: 'code',
      where: { kind: 'regex', value: 'use[A-Z]\\w+' },
    },
    {
      name: 'structural over code',
      target: 'code',
      where: {
        kind: 'structural',
        lang: 'ts',
        pattern: 'function $F($$$) { $$$ }',
      },
    },
    {
      name: 'text content over files',
      target: 'files',
      where: { kind: 'text', value: 'useEffect' },
    },
    {
      name: 'field extension = over files',
      target: 'files',
      where: { kind: 'field', field: 'extension', op: '=', value: 'ts' },
    },
    {
      name: 'field basename = over files',
      target: 'files',
      where: { kind: 'field', field: 'basename', op: '=', value: 'index.ts' },
    },
    {
      name: 'field path = over files',
      target: 'files',
      where: { kind: 'field', field: 'path', op: '=', value: 'src/index.ts' },
    },
    {
      name: 'field size > over files',
      target: 'files',
      where: { kind: 'field', field: 'size', op: '>', value: 1024 },
    },
  ];

  for (const c of CASES) {
    it(`${c.name}: transformer verdict agrees with plan routing`, () => {
      const input = {
        target: c.target,
        from: { kind: 'github', repo: 'facebook/react' },
        where: c.where,
        materialize: 'never',
      };
      const q = normalizeQuery(input as never) as OqlQuery;
      const { plan: p, executable } = planQuery(q, input);
      const transformed = toGithubCodeSearchToolQuery(q);

      // Executable all-PUSHDOWN plan => the provider transformer must accept.
      const allPushdown =
        executable && p.nodes.every(n => n.route === 'PUSHDOWN');
      if (allPushdown) expect(transformed.ok).toBe(true);
      // Transformer rejection => the planner must not have claimed an
      // executable pure-pushdown route for the same query.
      if (!transformed.ok) expect(allPushdown).toBe(false);
    });
  }
});

describe('inapplicable controls.search.sort warns instead of silently dropping', () => {
  const CASES: Array<{ target: string; sort: string; warns: boolean }> = [
    { target: 'files', sort: 'relevance', warns: true },
    { target: 'files', sort: 'size', warns: false },
    { target: 'code', sort: 'size', warns: true },
    { target: 'code', sort: 'relevance', warns: false },
  ];
  for (const c of CASES) {
    it(`${c.target} + sort:${c.sort} -> ${c.warns ? 'warning' : 'clean'}`, () => {
      const { plan: p } = plan({
        target: c.target,
        from: { kind: 'local', path: './src' },
        where: { kind: 'text', value: 'pagination' },
        controls: { search: { sort: c.sort } },
      });
      const warn = p.diagnostics.find(
        d =>
          d.code === 'lossyTransform' && d.queryPath === 'controls.search.sort'
      );
      if (c.warns) {
        expect(warn).toBeDefined();
        expect(warn?.severity).toBe('warning');
        expect(warn?.blocksAnswer).toBe(false);
        expect(warn?.message).toContain(`"${c.sort}"`);
      } else {
        expect(warn).toBeUndefined();
      }
    });
  }
});

describe('controls.search.sort on non-search targets warns (no silent drop)', () => {
  const CASES: Array<{
    target: string;
    params: Record<string, unknown>;
    passthroughHint: boolean;
  }> = [
    { target: 'repositories', params: { keywords: ['oql'] }, passthroughHint: true },
    { target: 'packages', params: { packageName: 'react' }, passthroughHint: true },
    { target: 'pullRequests', params: { state: 'merged' }, passthroughHint: false },
  ];
  for (const c of CASES) {
    it(`${c.target} + sort -> warning${c.passthroughHint ? ' with params.sort hint' : ''}`, () => {
      const { plan: p } = plan({
        target: c.target,
        from: { kind: 'github', repo: 'facebook/react' },
        params: c.params,
        controls: { search: { sort: 'relevance' } },
      });
      const warn = p.diagnostics.find(
        d =>
          d.code === 'lossyTransform' && d.queryPath === 'controls.search.sort'
      );
      expect(warn).toBeDefined();
      expect(warn?.severity).toBe('warning');
      expect(warn?.blocksAnswer).toBe(false);
      expect(warn?.message).toContain(`"relevance"`);
      expect(warn?.message).toContain(c.target);
      if (c.passthroughHint) {
        expect(warn?.message).toContain('params.sort');
      } else {
        expect(warn?.message).not.toContain('params.sort');
      }
    });
  }
});
