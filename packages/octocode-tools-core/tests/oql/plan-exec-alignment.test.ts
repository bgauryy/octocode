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
