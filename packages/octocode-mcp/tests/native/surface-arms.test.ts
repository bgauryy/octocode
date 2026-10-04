// Catalog-shape switches (S13 arms) the native catalog reports under
// `presentation`: the flat published view and deferred tools behind the
// `run` dispatcher. Defaults keep the current catalog.
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
import { wrapBareQuery } from './wrapBareQuery.js';

const executions: { tool: string; input: unknown }[] = [];

function bindingFor(catalog: NativeCatalog) {
  return {
    NativeRuntime: class implements NativeRuntime {
      readonly abiVersion = NATIVE_ABI_VERSION;
      catalog(): NativeCatalog {
        return catalog;
      }
      normalizeInput(_tool: string, input: unknown): unknown {
        return wrapBareQuery(input);
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
  'ghSearchRepo',
  'ghSearchCode',
  TOOL_NAMES.CLASIFY,
];

async function connect(presentation?: NativeCatalog['presentation']) {
  const instance = createNativeMcp({
    env: {},
    binding: bindingFor({
      fingerprint: getNativeContractFingerprint(),
      tools: TOOLS.map(name => ({ name, available: true })),
      ...(presentation && { presentation }),
    }),
  });
  const client = new Client({ name: 'surface-arms', version: '1' });
  const [serverTransport, clientTransport] =
    InMemoryTransport.createLinkedPair();
  await Promise.all([
    instance.server.connect(serverTransport),
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

describe('flat published view', () => {
  it('lists one row per tool, never the queries wrapper', async () => {
    const control = await connect();
    const flat = await connect({ publishedView: 'flat' });
    expect(flat.listed.map(t => t.name)).toEqual(
      control.listed.map(t => t.name)
    );
    expect(JSON.stringify(flat.listed)).not.toMatch(/queries/);
    expect(flat.instructions).not.toMatch(/queries/);
    expect(JSON.stringify(flat.listed).length).toBeLessThan(
      JSON.stringify(control.listed).length
    );
    // A flat row is the advertised call; both shapes still execute.
    const row = { path: 'src', searchText: 'x' };
    expect(
      (await flat.client.callTool({ name: 'localSearch', arguments: row }))
        .isError
    ).toBe(false);
    expect(
      (
        await flat.client.callTool({
          name: 'localSearch',
          arguments: { queries: [row] },
        })
      ).isError
    ).toBe(false);
    // Validation applies schema defaults; the row reaches native wrapped.
    expect(executions.map(e => e.input)).toMatchObject([
      { queries: [row] },
      { queries: [row] },
    ]);
    await control.close();
    await flat.close();
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
    expect(deferred.instructions).toMatch(
      /run runs ghSearchRepo, clasify and every lead naming them\./
    );
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
        input: { queries: [{ keywords: ['octocode'] }] },
      },
    ]);
    expect(executions).toHaveLength(1);
    await deferred.close();
  });

  it('validates the query against the target schema with actionable errors', async () => {
    const deferred = await connect(presentation);
    const response = await deferred.client.callTool({
      name: DEFERRED_TOOL_DISPATCHER,
      arguments: { tool: 'ghSearchRepo', query: { keywords: 'x', bogus: 1 } },
    });
    expect(response.isError).toBe(true);
    expect(JSON.stringify(response.content)).toMatch(
      /Input validation error: Invalid arguments for tool ghSearchRepo/
    );
    const unknown = await deferred.client.callTool({
      name: DEFERRED_TOOL_DISPATCHER,
      arguments: { tool: 'astRewrite', query: {} },
    });
    expect(unknown.isError).toBe(true);
    expect(executions).toEqual([]);
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
    const none = await connect({ deferred: ['astRewrite'] });
    expect(none.listed.map(t => t.name)).toEqual(
      control.listed.map(t => t.name)
    );
    expect(none.instructions).toBe(control.instructions);
    await control.close();
    await none.close();
  });
});
