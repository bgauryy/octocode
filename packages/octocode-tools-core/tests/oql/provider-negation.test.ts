import { describe, expect, it } from 'vitest';
import { normalizeQuery } from '../../src/oql/normalize.js';
import { planQuery } from '../../src/oql/planner.js';
import { runOqlSearch } from '../../src/oql/run.js';
import { isBatchEnvelope, type OqlQuery } from '../../src/oql/types.js';

function plan(input: unknown) {
  const q = normalizeQuery(input as never) as OqlQuery;
  return planQuery(q, input);
}
function single(r: Awaited<ReturnType<typeof runOqlSearch>>) {
  if (isBatchEnvelope(r)) throw new Error('expected single');
  return r;
}

/**
 * Gate 15: `not`/`xor` over a GitHub provider source cannot return `proof`
 * unless the candidate universe is complete (i.e. materialized for local
 * proof). The provider can never prove *absence* of a match.
 */
describe('OQL gate 15: negation over a GitHub provider source', () => {
  it('not(text) + materialize:never -> UNSUPPORTED + negativeUniverseRequired', () => {
    const { plan: p, executable } = plan({
      target: 'files',
      from: { kind: 'github', repo: 'facebook/react' },
      scope: { path: 'packages/react' },
      where: { kind: 'not', predicate: { kind: 'text', value: 'useEffect' } },
      materialize: 'never',
    });
    expect(executable).toBe(false);
    expect(p.diagnostics.some(d => d.code === 'negativeUniverseRequired')).toBe(
      true
    );
  });

  it('not(text) + materialize:auto -> ROUTE to local proof (never exact provider negation)', () => {
    const { plan: p, executable } = plan({
      target: 'files',
      from: { kind: 'github', repo: 'facebook/react' },
      scope: { path: 'packages/react' },
      where: { kind: 'not', predicate: { kind: 'text', value: 'useEffect' } },
      materialize: { mode: 'auto', strategy: 'subtree' },
    });
    expect(executable).toBe(true);
    // The `not` node must route to materialization, not push down to provider.
    const notNode = p.nodes.find(n => n.path === 'where');
    expect(notNode?.route).toBe('ROUTE');
    // No backend call may claim it proved a negation directly via the provider.
    expect(
      p.backendCalls.some(c => c.backend === 'ghSearchCode' && c.exact)
    ).toBe(false);
    // The plan must reach a local proof backend after materialization.
    expect(
      p.backendCalls.some(
        c => c.backend === 'localSearchCode' || c.backend === 'localFindFiles'
      )
    ).toBe(true);
  });

  it('xor + materialize:never over github -> not executable (cannot prove both branches)', () => {
    const { plan: p, executable } = plan({
      target: 'code',
      from: { kind: 'github', repo: 'facebook/react' },
      scope: { path: 'packages/react' },
      xor: [
        { kind: 'text', value: 'a' },
        { kind: 'text', value: 'b' },
      ],
      materialize: 'never',
    });
    expect(executable).toBe(false);
    expect(p.diagnostics.some(d => d.code === 'negativeUniverseRequired')).toBe(
      true
    );
  });

  it('xor + materialize:auto over github -> routes through materialization (some ROUTE node)', () => {
    const { plan: p, executable } = plan({
      target: 'code',
      from: { kind: 'github', repo: 'facebook/react' },
      scope: { path: 'packages/react' },
      xor: [
        { kind: 'text', value: 'a' },
        { kind: 'text', value: 'b' },
      ],
      materialize: { mode: 'auto', strategy: 'subtree' },
    });
    expect(executable).toBe(true);
    expect(p.nodes.some(n => n.route === 'ROUTE')).toBe(true);
  });

  it('double negation collapses to a positive provider search (exact, no materialization needed)', () => {
    const { plan: p, executable } = plan({
      target: 'files',
      from: { kind: 'github', repo: 'facebook/react' },
      scope: { path: 'packages/react' },
      where: {
        kind: 'not',
        predicate: {
          kind: 'not',
          predicate: { kind: 'text', value: 'useEffect' },
        },
      },
      materialize: 'never',
    });
    expect(executable).toBe(true);
    expect(p.diagnostics.some(d => d.code === 'negativeUniverseRequired')).toBe(
      false
    );
  });

  it('runOqlSearch surfaces unsupported evidence for a github negation without a universe', async () => {
    const env = single(
      await runOqlSearch({
        target: 'files',
        from: { kind: 'github', repo: 'facebook/react' },
        scope: { path: 'packages/react' },
        where: { kind: 'not', predicate: { kind: 'text', value: 'useEffect' } },
        materialize: { mode: 'never' },
      })
    );
    expect(env.evidence.kind).toBe('unsupported');
    expect(env.evidence.answerReady).toBe(false);
  });
});
