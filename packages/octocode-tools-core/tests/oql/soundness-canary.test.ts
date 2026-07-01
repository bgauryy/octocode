/**
 * Backend-soundness canaries: known-answer queries against a committed
 * fixture, run through the REAL backends (no mocks). OQL's evidence model
 * labels backend results as proof — it cannot detect a backend that starts
 * returning silently-wrong zeros (as composed inside+has structural rules
 * did for an entire rule class). These canaries fail loudly instead.
 */
import { describe, expect, it } from 'vitest';
import nodePath from 'node:path';
import { runOqlSearch } from '../../src/oql/run.js';
import {
  isBatchEnvelope,
  type OqlResultEnvelope,
} from '../../src/oql/types.js';

const FIXTURE_DIR = nodePath.join(__dirname, 'fixtures');

async function run(query: Record<string, unknown>): Promise<OqlResultEnvelope> {
  const env = await runOqlSearch({
    from: { kind: 'local', path: FIXTURE_DIR },
    scope: { include: ['**/soundness-canary.ts'] },
    ...query,
  } as never);
  if (isBatchEnvelope(env)) throw new Error('expected single envelope');
  return env;
}

describe('backend-soundness canaries (real engine, known answers)', () => {
  it('text: CANARY_TOKEN_A matches exactly 3 lines with proof', async () => {
    const env = await run({
      target: 'code',
      where: { kind: 'text', value: 'CANARY_TOKEN_A' },
    });
    expect(env.results).toHaveLength(3);
    expect(env.evidence.kind).toBe('proof');
  });

  it('regex: exported canary functions match exactly 2 declarations', async () => {
    const env = await run({
      target: 'code',
      where: { kind: 'regex', value: '^export function canary\\w+' },
    });
    expect(env.results).toHaveLength(2);
    expect(env.evidence.kind).toBe('proof');
  });

  it('structural pattern with return type matches the typed functions', async () => {
    const env = await run({
      target: 'code',
      where: {
        kind: 'structural',
        lang: 'ts',
        pattern: 'function $N($$$A): $R { $$$B }',
      },
    });
    expect(env.results.length).toBeGreaterThan(0);
  });

  it('CANARY: composed inside+has structural rule returns matches', async () => {
    // This exact rule class silently returned 0 for months (engine capture
    // collision). If this drops to zero again, a backend regressed — the
    // evidence model alone cannot see it.
    const env = await run({
      target: 'code',
      where: {
        kind: 'structural',
        lang: 'ts',
        rule: {
          kind: 'call_expression',
          inside: {
            kind: 'function_declaration',
            has: { pattern: 'canaryConsumer' },
            stopBy: 'end',
          },
        },
      },
    });
    expect(env.results.length).toBeGreaterThan(0);
    expect(
      env.results.some(r =>
        'snippet' in r
          ? String(r.snippet).includes('canaryTypedFunction')
          : false
      )
    ).toBe(true);
  });

  it('files: extension equality finds the fixture', async () => {
    const env = await run({
      target: 'files',
      where: {
        kind: 'field',
        field: 'basename',
        op: '=',
        value: 'soundness-canary.ts',
      },
    });
    expect(env.results).toHaveLength(1);
    expect(env.evidence.kind).toBe('proof');
  });
});
