import { afterEach, describe, expect, it, vi } from 'vitest';
import { getNativeContractFingerprint } from '@octocodeai/config/schema';
import { runScheme } from '../../../src/cli/commands/scheme.js';

const machine = vi.hoisted(() => ({
  fingerprint: '',
  tools: [] as Array<{ name: string; availability: { enabled: boolean } }>,
}));

vi.mock('../../../src/cli/native-delegate.js', () => ({
  resolveNativeBin: () => '/test/octocode',
}));
vi.mock('node:child_process', async () => {
  const { promisify } = await import('node:util');
  return {
    execFile: Object.assign(vi.fn(), {
      [promisify.custom]: async () => ({ stdout: JSON.stringify(machine) }),
    }),
  };
});

afterEach(() => vi.restoreAllMocks());

describe('scheme availability-scoped guidance', () => {
  it.each([
    ['localSearch', 'full', false],
    ['localSearch', 'query', false],
    ['ghSearch', 'full', false],
    ['ghSearch', 'query', false],
    ['localSearch', 'full', true],
    ['ghSearch', 'full', true],
  ] as const)(
    '%s %s respects clasify enabled=%s',
    async (tool, view, enabled) => {
      machine.fingerprint = getNativeContractFingerprint();
      machine.tools = [
        'localSearch',
        'localFetch',
        'ghSearch',
        'ghGetFileContent',
        'clasify',
      ].map(name => ({
        name,
        availability: { enabled: name !== 'clasify' || enabled },
      }));
      const output = vi.spyOn(console, 'log').mockImplementation(() => {});
      await expect(
        runScheme({
          command: 'scheme',
          args: [tool],
          options: { view, compact: true },
        })
      ).resolves.toBe(0);
      const result = JSON.parse(String(output.mock.calls[0]?.[0]));
      const description = result.description ?? result.querySchema.description;
      expect(description.includes('clasify')).toBe(enabled);
      expect(
        JSON.stringify(result.querySchema).includes('semanticRerank')
      ).toBe(enabled);
    }
  );
});
