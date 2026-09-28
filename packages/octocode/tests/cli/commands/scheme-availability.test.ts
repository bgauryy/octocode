import { afterEach, describe, expect, it, vi } from 'vitest';
import { getNativeContractFingerprint } from '@octocodeai/config/schema';
import {
  runScheme,
  printAgentInstructions,
} from '../../../src/cli/commands/scheme.js';

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

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllEnvs();
});

describe('scheme contract-drift gate', () => {
  it.each([
    ['development', 0],
    ['production', 1],
  ] as const)(
    'OCTOCODE_ALLOW_CONTRACT_DRIFT with NODE_ENV=%s exits %s',
    async (nodeEnv, exit) => {
      machine.fingerprint = 'f'.repeat(64);
      machine.tools = [
        { name: 'localSearch', availability: { enabled: true } },
      ];
      vi.stubEnv('OCTOCODE_ALLOW_CONTRACT_DRIFT', '1');
      vi.stubEnv('NODE_ENV', nodeEnv);
      const output = vi.spyOn(console, 'log').mockImplementation(() => {});
      vi.spyOn(console, 'error').mockImplementation(() => {});
      const code = await runScheme({
        command: 'scheme',
        args: [],
        options: { compact: true, 'json-errors': true },
      });
      expect(code === 0 ? 0 : 1).toBe(exit);
      const printed = JSON.parse(String(output.mock.calls[0]?.[0]));
      if (exit === 1) {
        expect(printed.kind).toBe('octocode.toolError');
        expect(printed.error).toContain('Contract drift');
      } else {
        expect(printed.kind).toBe('octocode.toolCatalog');
      }
    }
  );
});

describe('scheme availability-scoped guidance', () => {
  it('restricts Clasify delegated reads to enabled tools in the query schema', async () => {
    machine.fingerprint = getNativeContractFingerprint();
    machine.tools = ['clasify', 'ghSearchCode', 'ghGetFileContent'].map(
      name => ({
        name,
        availability: { enabled: true },
      })
    );
    const output = vi.spyOn(console, 'log').mockImplementation(() => {});
    expect(
      await runScheme({
        command: 'scheme',
        args: ['clasify'],
        options: { view: 'query', compact: true },
      })
    ).toBe(0);
    const schema = JSON.stringify(
      JSON.parse(String(output.mock.calls[0]?.[0])).querySchema
    );
    expect(schema).toContain('"ghGetFileContent"');
    expect(schema).not.toContain('"astTopology"');
    expect(schema).not.toContain('"localFetch"');
  });

  it.each([
    ['localSearch', 'full', false],
    ['localSearch', 'query', false],
    ['ghSearchCode', 'full', false],
    ['ghSearchCode', 'query', false],
    ['localSearch', 'full', true],
    ['ghSearchCode', 'full', true],
  ] as const)(
    '%s %s respects clasify enabled=%s',
    async (tool, view, enabled) => {
      machine.fingerprint = getNativeContractFingerprint();
      machine.tools = [
        'localSearch',
        'localFetch',
        'ghSearchCode',
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
      expect(description.includes('semanticRerank')).toBe(false);
      const schema = view === 'full' ? result.inputSchema : result.querySchema;
      expect(JSON.stringify(schema).includes('semanticRerank')).toBe(false);
    }
  );
});

describe('root help instructions', () => {
  it.each([false, true])(
    'matches catalog instructions with clasify enabled=%s',
    async enabled => {
      machine.fingerprint = getNativeContractFingerprint();
      machine.tools = ['localSearch', 'localFetch', 'clasify'].map(name => ({
        name,
        availability: { enabled: name !== 'clasify' || enabled },
      }));
      const output = vi.spyOn(console, 'log').mockImplementation(() => {});
      expect(
        await runScheme({
          command: 'scheme',
          args: [],
          options: { compact: true },
        })
      ).toBe(0);
      const catalog = JSON.parse(String(output.mock.calls[0]?.[0]));
      expect(catalog.commands.schema).toBe('scheme <name> --view query');
      expect(catalog.commands.fullContract).toBe('scheme <name> --view full');
      output.mockClear();
      expect(await printAgentInstructions()).toBe(0);
      expect(output).toHaveBeenCalledWith(
        `\nAgent instructions:\n${catalog.instructions}`
      );
      expect(catalog.instructions.includes('clasify')).toBe(enabled);
    }
  );

  it('refuses drifted help instructions in production', async () => {
    machine.fingerprint = 'f'.repeat(64);
    vi.stubEnv('NODE_ENV', 'production');
    vi.stubEnv('OCTOCODE_ALLOW_CONTRACT_DRIFT', '1');
    const output = vi.spyOn(console, 'log').mockImplementation(() => {});
    vi.spyOn(console, 'error').mockImplementation(() => {});
    expect(await printAgentInstructions()).toBe(5);
    expect(output).not.toHaveBeenCalled();
  });
});
