// Catalog-shape switches the native catalog reports under `presentation`:
// deferred tools behind the `run` dispatcher. Defaults keep the current
// catalog.
import { afterEach, describe, expect, it } from 'vitest';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/server';
import {
  getNativeContractFingerprint,
  TOOL_NAMES,
} from '@octocodeai/config/schema';
import { DEFERRED_TOOL_DISPATCHER } from '@octocodeai/config/mcp';
import { NATIVE_ABI_VERSION } from '@octocodeai/octocode-native/runtime';
import {
  createNativeMcp,
  type NativeCatalog,
  type NativeRuntime,
} from '../../src/native/index.js';

const executions: { tool: string; input: unknown }[] = [];

function bindingFor(catalog: NativeCatalog) {
  return {
    NativeRuntime: class implements NativeRuntime {
      readonly abiVersion = NATIVE_ABI_VERSION;
      catalog(): NativeCatalog {
        return catalog;
      }
      cancel(): boolean {
        return true;
      }
      async executeMcp(_id: string, tool: string, input: unknown) {
        executions.push({ tool, input });
        return {
          content: [{ type: 'text', text: 'ok' }],
          structuredContent: { results: [{ index: 0, data: { ok: true } }] },
          isError: false,
        };
      }
      async close(): Promise<void> {}
    },
  };
}

const TOOLS = [
  'localSearch',
  'localFetch',
  'ghGetFileContent',
  'structureSearch',
  'astSearch',
  'lspSearch',
  'ghSearchRepo',
  'ghSearchCode',
  'ghSearchHistory',
  'ghGetHistoryItem',
  'ghStructure',
  'artifactSearch',
  TOOL_NAMES.CLASIFY,
];

async function connect(
  presentation?: NativeCatalog['presentation'],
  available = TOOLS
) {
  const instance = await createNativeMcp({
    env: {},
    binding: bindingFor({
      fingerprint: getNativeContractFingerprint(),
      tools: available.map(name => ({
        name,
        shortDescription: `${name} description.`,
        available: true,
      })),
      ...(presentation && { presentation }),
    }),
  });
  const client = new Client({ name: 'surface-arms', version: '1' });
  const [serverTransport, clientTransport] =
    InMemoryTransport.createLinkedPair();
  await Promise.all([
    instance.createServer().connect(serverTransport),
    client.connect(clientTransport),
  ]);
  const listed = (await client.listTools()).tools;
  return {
    client,
    listed,
    instructions: client.getInstructions() ?? '',
    close: async () => {
      await client.close();
      await instance.close();
    },
  };
}

afterEach(() => {
  executions.length = 0;
});

function queryRow(schema: { properties?: Record<string, unknown> }) {
  return (
    schema.properties?.queries as {
      items: { properties: Record<string, unknown>; required?: string[] };
    }
  ).items;
}

describe('read-tool selection schema', () => {
  it.each([
    { selection: false, clasifyListed: false, shown: false },
    { selection: false, clasifyListed: true, shown: false },
    { selection: true, clasifyListed: false, shown: true },
    { selection: true, clasifyListed: true, shown: true },
  ])(
    'uses native selection=$selection independently of public clasify=$clasifyListed',
    async ({ selection, clasifyListed, shown }) => {
      const server = await connect(
        { selection },
        TOOLS.filter(name => clasifyListed || name !== TOOL_NAMES.CLASIFY)
      );
      try {
        expect(
          server.listed.some(tool => tool.name === TOOL_NAMES.CLASIFY)
        ).toBe(clasifyListed);
        expect(server.instructions.includes('<clasify>')).toBe(
          selection || clasifyListed
        );
        expect(server.instructions.includes('clasifyContext')).toBe(selection);
        expect(server.instructions.includes('clasify scouts unread')).toBe(
          clasifyListed
        );
        for (const name of TOOLS.filter(
          name => !['clasify', 'ghCloneRepo'].includes(name)
        )) {
          const schema = queryRow(
            server.listed.find(tool => tool.name === name)!.inputSchema
          );
          const supportsSelection = [
            'localFetch',
            'localSearch',
            'ghGetFileContent',
            'ghSearchCode',
          ].includes(name);
          expect(
            JSON.stringify(schema).includes('"clasifyContext"'),
            name
          ).toBe(shown && supportsSelection);
          expect(schema.required ?? []).not.toContain('clasifyContext');
        }
      } finally {
        await server.close();
      }
    }
  );

  it('keeps clasifyContext exposed when available clasify is deferred', async () => {
    const server = await connect({
      selection: true,
      deferred: [TOOL_NAMES.CLASIFY],
    });
    try {
      expect(
        queryRow(
          server.listed.find(tool => tool.name === 'localFetch')!.inputSchema
        ).properties
      ).toHaveProperty('clasifyContext');
    } finally {
      await server.close();
    }
  });
});

describe('deferred tools', () => {
  const presentation = { deferred: [TOOL_NAMES.CLASIFY, 'ghSearchRepo'] };

  it('hides them from tools/list behind one run dispatcher', async () => {
    const control = await connect();
    const deferred = await connect(presentation);
    const names = deferred.listed.map(t => t.name);
    expect(names).not.toContain('ghSearchRepo');
    expect(names).not.toContain(TOOL_NAMES.CLASIFY);
    expect(names.at(-1)).toBe(DEFERRED_TOOL_DISPATCHER);
    expect(JSON.stringify(deferred.listed).length).toBeLessThan(
      JSON.stringify(control.listed).length
    );
    const run = deferred.listed.find(t => t.name === DEFERRED_TOOL_DISPATCHER)!;
    expect(run.inputSchema.properties).toMatchObject({
      tool: { enum: ['ghSearchRepo', TOOL_NAMES.CLASIFY] },
    });
    // The dispatcher description carries the route; the prompt is unchanged.
    expect(run.description).toMatch(/^Run a tool not listed here/);
    expect(deferred.instructions).toBe(control.instructions);
    await control.close();
    await deferred.close();
  });

  it('runs a {tool, query} lead verbatim through native', async () => {
    const deferred = await connect(presentation);
    const lead = {
      tool: 'ghSearchRepo',
      confidence: 'medium',
      query: { keywords: ['octocode'] },
    };
    const response = await deferred.client.callTool({
      name: DEFERRED_TOOL_DISPATCHER,
      arguments: lead,
    });
    expect(response.isError).toBe(false);
    expect(executions).toMatchObject([
      {
        tool: 'ghSearchRepo',
        input: { keywords: ['octocode'] },
      },
    ]);
    expect(executions).toHaveLength(1);
    await deferred.close();
  });

  it('hands the tool and query to native, the only validator', async () => {
    const deferred = await connect(presentation);
    await deferred.client.callTool({
      name: DEFERRED_TOOL_DISPATCHER,
      arguments: { tool: 'ghSearchRepo', query: { keywords: 'x', bogus: 1 } },
    });
    expect(executions.at(-1)).toEqual({
      tool: 'ghSearchRepo',
      input: { keywords: 'x', bogus: 1 },
    });
    // An unknown or unavailable tool is native's typed tool error too.
    await deferred.client.callTool({
      name: DEFERRED_TOOL_DISPATCHER,
      arguments: { tool: 'notATool', query: {} },
    });
    expect(executions.at(-1)).toEqual({ tool: 'notATool', input: {} });
    await deferred.close();
  });

  it('also runs a lead to a listed tool, so a copied lead never fails on routing', async () => {
    const deferred = await connect(presentation);
    const response = await deferred.client.callTool({
      name: DEFERRED_TOOL_DISPATCHER,
      arguments: { tool: 'localFetch', query: { path: 'a.ts' } },
    });
    expect(response.isError).toBe(false);
    expect(executions.at(-1)?.tool).toBe('localFetch');
    await deferred.close();
  });

  it('ignores deferral of an unavailable tool', async () => {
    const control = await connect();
    const none = await connect({ deferred: [] });
    expect(none.listed.map(t => t.name)).toEqual(
      control.listed.map(t => t.name)
    );
    expect(none.instructions).toBe(control.instructions);
    await control.close();
    await none.close();
  });
});
