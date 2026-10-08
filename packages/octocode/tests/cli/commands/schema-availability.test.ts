import { afterEach, describe, expect, it, vi } from 'vitest';
import { getNativeContractFingerprint } from '@octocodeai/config/schema';
import { buildCliInstructions } from '@octocodeai/config/mcp';
import { runSchema } from '../../../src/cli/commands/schema.js';

const machine = vi.hoisted(() => ({
  fingerprint: '',
  tools: [] as Array<{ name: string; availability: { enabled: boolean } }>,
}));

vi.mock('../../../src/cli/native-delegate.js', async importOriginal => ({
  ...(await importOriginal<
    typeof import('../../../src/cli/native-delegate.js')
  >()),
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

describe('schema contract-drift gate', () => {
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
      const code = await runSchema({
        command: 'schema',
        args: [],
        options: {},
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

describe('schema availability-scoped guidance', () => {
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
      await runSchema({
        command: 'schema',
        args: ['clasify'],
        options: { view: 'query' },
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
        runSchema({
          command: 'schema',
          args: [tool],
          options: { view },
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

describe('catalog instructions', () => {
  it.each([false, true])(
    'serve one prompt whether clasify is enabled or not (enabled=%s)',
    async enabled => {
      machine.fingerprint = getNativeContractFingerprint();
      machine.tools = ['localSearch', 'localFetch', 'clasify'].map(name => ({
        name,
        availability: { enabled: name !== 'clasify' || enabled },
      }));
      const output = vi.spyOn(console, 'log').mockImplementation(() => {});
      expect(
        await runSchema({ command: 'schema', args: [], options: {} })
      ).toBe(0);
      const catalog = JSON.parse(String(output.mock.calls[0]?.[0]));
      expect(catalog.commands).toEqual({
        run: "octocode <tool> '<json>' (or --input FILE|-)",
        query: 'octocode schema <tool> --view query [--select FIELD=VALUE]',
        variants: 'octocode schema <tool> --view variants',
        full: 'octocode schema <tool>',
      });
      // The prompt says in plain words that clasify needs a key.
      expect(catalog.instructions).toBe(buildCliInstructions());
    }
  );

  it.each([false, true])(
    'lists only enabled tools (clasify enabled=%s)',
    async enabled => {
      machine.fingerprint = getNativeContractFingerprint();
      machine.tools = ['localSearch', 'localFetch', 'clasify'].map(name => ({
        name,
        availability: { enabled: name !== 'clasify' || enabled },
      }));
      const output = vi.spyOn(console, 'log').mockImplementation(() => {});
      vi.spyOn(console, 'error').mockImplementation(() => {});
      await runSchema({ command: 'schema', args: [], options: {} });
      const printed = JSON.parse(String(output.mock.calls[0]?.[0]));
      expect(
        printed.tools.some((tool: { name: string }) => tool.name === 'clasify')
      ).toBe(enabled);
      expect(printed.toolCount).toBe(enabled ? 3 : 2);
      output.mockClear();
      // Not a terminal: the error is the JSON envelope on stdout.
      await runSchema({ command: 'schema', args: ['nope'], options: {} });
      const error = JSON.parse(String(output.mock.calls[0]?.[0]));
      expect(error.kind).toBe('octocode.toolError');
      expect(String(error.error).includes('clasify')).toBe(enabled);
    }
  );
});
