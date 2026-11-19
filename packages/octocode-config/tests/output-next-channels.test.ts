import { describe, expect, it } from 'vitest';
import { buildEnforcementContractIr, DIRECT_TOOL_DEFINITIONS } from '@octocodeai/octocode-core/schema';
import { at, tool } from './cli-presentation-fixture.ts';

type Registry = Readonly<Record<string, { readonly row: Readonly<Record<string, unknown>> }>>;

/** The key a channel is looked up by in the output reference: `cycle<n>` when numbered. */
function referenceKey(name: string, definition: unknown): string {
  const kind =
    typeof definition === 'object' && definition !== null && 'kind' in definition ? definition.kind : definition;
  return kind === 'numbered' ? `${name}<n>` : name;
}

describe('output reference next channels (pkg-13)', () => {
  it('states the meaning of every next channel a tool can emit, after next itself', () => {
    const ir = buildEnforcementContractIr();
    const registry: Registry = ir.continuationChannels;
    const errors = Object.keys(ir.errorContinuationChannels);
    for (const { name } of DIRECT_TOOL_DEFINITIONS) {
      const output = at(tool(name), 'output');
      const channels = [
        ...Object.entries(registry[name]?.row ?? {}).map(([channel, definition]) => referenceKey(channel, definition)),
        ...errors,
      ].map(channel => `next.${channel}`);
      for (const key of channels) {
        const meaning = output[key];
        expect(typeof meaning === 'string' && meaning.trim() !== '', `${name} ${key}`).toBe(true);
      }
      const keys = Object.keys(output);
      const nextAt = keys.indexOf('next');
      expect(nextAt, `${name} documents next`).toBeGreaterThanOrEqual(0);
      expect(keys.slice(nextAt + 1, nextAt + 1 + channels.length), name).toEqual(channels);
    }
  });

  it("says what clasify's read lead reads", () => {
    const output = at(tool('clasify'), 'output');
    expect(output['next.read']).toMatch(/deciding lines/);
    expect(output['next.confirm']).toMatch(/maxRequests raised to its requestsCeiling/);
    expect(Object.keys(output).filter(key => key.startsWith('next.'))).toEqual([
      'next.nextPage',
      'next.quiet',
      'next.details',
      'next.confirm',
      'next.read',
      'next.retry',
      'next.restart',
    ]);
  });
});
