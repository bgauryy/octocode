import { describe, expect, it, vi } from 'vitest';
import type {
  CallHierarchyItem,
  IncomingCall,
  OutgoingCall,
} from '@octocodeai/octocode-engine/lsp/types';
import {
  callsEnvelope,
  typeHierarchyEnvelope,
} from '../../../src/tools/lsp/semantic_content/semanticEnvelopes/callEnvelopes.js';
import { compactSemanticEnvelope } from '../../../src/tools/lsp/semantic_content/semanticPresentation.js';
import { withSemanticNext } from '../../../src/tools/lsp/semantic_content/semanticNext.js';

function item(name: string, line: number): CallHierarchyItem {
  const range = { start: { line, character: 0 }, end: { line, character: 10 } };
  return {
    name,
    kind: 12,
    uri: 'file:///repo/source.ts',
    range,
    selectionRange: range,
  };
}
const roots = [item('first', 0), item('second', 1)];
const target = item('unique', 8);
const anchor = {
  uri: roots[0]!.uri,
  absolutePath: '/repo/source.ts',
  content: '',
  resolvedSymbol: {
    name: 'target',
    uri: roots[0]!.uri,
    foundAtLine: 1,
    range: roots[0]!.range,
    position: { line: 0, character: 0 },
  },
} as Parameters<typeof callsEnvelope>[1];

function client(prepared = roots) {
  return {
    prepareCallHierarchy: vi.fn().mockResolvedValue(prepared),
    getIncomingCalls: vi
      .fn<(item: CallHierarchyItem) => Promise<IncomingCall[]>>()
      .mockResolvedValue([]),
    getOutgoingCalls: vi
      .fn<(item: CallHierarchyItem) => Promise<OutgoingCall[]>>()
      .mockResolvedValue([]),
  };
}
async function execute(
  operation: 'callers' | 'callees' | 'callHierarchy',
  provider: ReturnType<typeof client>
) {
  const query = {
    operation,
    uri: anchor.uri,
    symbolName: 'target',
    lineHint: 1,
    depth: 1,
  };
  return callsEnvelope(
    query,
    anchor,
    provider as unknown as Parameters<typeof callsEnvelope>[2]
  );
}

describe('call hierarchy prepared roots', () => {
  it.each(['callers', 'callees'] as const)(
    'retains %s found only under the second prepared root',
    async operation => {
      const provider = client();
      provider.getIncomingCalls.mockImplementation(async root =>
        root.name === 'second'
          ? [{ from: target, fromRanges: [target.range] }]
          : []
      );
      provider.getOutgoingCalls.mockImplementation(async root =>
        root.name === 'second'
          ? [{ to: target, fromRanges: [target.range] }]
          : []
      );
      const result = await execute(operation, provider);
      expect(JSON.stringify(result.payload)).toContain('unique');
      expect(result.payload).toMatchObject({
        roots: [{ name: 'first' }, { name: 'second' }],
      });
      expect(compactSemanticEnvelope(result).payload).toMatchObject({
        roots: [
          expect.stringContaining('first'),
          expect.stringContaining('second'),
        ],
      });
    }
  );

  it('deduplicates identical calls across roots while retaining distinct source ranges', async () => {
    const provider = client();
    provider.getIncomingCalls.mockImplementation(async root => [
      { from: target, fromRanges: [target.range] },
      ...(root.name === 'second'
        ? [{ from: target, fromRanges: [roots[1]!.range] }]
        : []),
    ]);
    const result = await execute('callers', provider);
    expect(result.payload).toMatchObject({ incomingCalls: 2 });
  });

  it('marks a failed prepared root incomplete', async () => {
    const provider = client();
    provider.getIncomingCalls.mockImplementation(async root => {
      if (root.name === 'second') throw new Error('provider failed');
      return [];
    });
    const result = await execute('callers', provider);
    expect(result.payload).toMatchObject({
      completeness: { complete: false, failedRequestCount: 1 },
    });
    expect(result.incompleteResults).toBe(true);
  });

  it('shares the existing request cap across prepared roots and reports its terminal limit', async () => {
    const provider = client(
      Array.from({ length: 60 }, (_, index) => item(`root${index}`, index))
    );
    const result = await execute('callers', provider);
    expect(provider.getIncomingCalls).toHaveBeenCalledTimes(50);
    expect(result.payload).toMatchObject({
      completeness: {
        complete: false,
        truncatedByBudget: true,
        requestCount: 50,
        preparedRootCount: 60,
      },
    });
    expect(
      withSemanticNext(
        {
          operation: 'callers',
          uri: anchor.uri,
          symbolName: 'target',
          lineHint: 1,
        },
        result
      )
    ).toMatchObject({
      terminalLimit: true,
      partialReasons: expect.arrayContaining(['budget']),
    });
  });
});

describe('type hierarchy prepared roots', () => {
  function provider(prepared = roots) {
    return {
      prepareTypeHierarchy: vi.fn().mockResolvedValue(prepared),
      typeHierarchySupertypes: vi
        .fn<(root: unknown) => Promise<unknown[]>>()
        .mockResolvedValue([]),
      typeHierarchySubtypes: vi
        .fn<(root: unknown) => Promise<unknown[]>>()
        .mockResolvedValue([]),
    };
  }
  async function run(
    operation: 'supertypes' | 'subtypes',
    client: ReturnType<typeof provider>
  ) {
    return typeHierarchyEnvelope(
      { operation, uri: anchor.uri, symbolName: 'target', lineHint: 1 },
      anchor,
      client as unknown as Parameters<typeof typeHierarchyEnvelope>[2]
    );
  }

  it.each(['supertypes', 'subtypes'] as const)(
    'preserves %s unique to the second prepared root',
    async operation => {
      const client = provider();
      const relation = async (root: unknown) =>
        root === roots[1] ? [target] : [];
      client.typeHierarchySupertypes.mockImplementation(relation);
      client.typeHierarchySubtypes.mockImplementation(relation);
      const result = await run(operation, client);
      expect(result.payload).toMatchObject({
        kind: 'typeHierarchy',
        roots,
        items: [target],
        totalItems: 1,
      });
      expect(compactSemanticEnvelope(result).payload).toMatchObject({ roots });
    }
  );

  it('deduplicates shared relative source anchors and retains failures as partial evidence', async () => {
    const client = provider([...roots, item('failing', 2)]);
    client.typeHierarchySupertypes.mockImplementation(async root => {
      if ((root as CallHierarchyItem).name === 'failing')
        throw new Error('provider failure');
      return [
        { ...target, data: { preparedRoot: (root as CallHierarchyItem).name } },
      ];
    });
    const result = await run('supertypes', client);
    expect(result.payload).toMatchObject({
      totalItems: 1,
      completeness: { complete: false, failedRequestCount: 1 },
    });
    expect(result.incompleteResults).toBe(true);
  });

  it('does not report all failed roots as an absence of types', async () => {
    const client = provider();
    client.typeHierarchySubtypes.mockRejectedValue(
      new Error('provider failure')
    );
    const result = await run('subtypes', client);
    expect(result.incompleteResults).toBe(true);
    expect(result.payload).toMatchObject({
      kind: 'typeHierarchy',
      items: [],
      roots,
      completeness: { complete: false, failedRequestCount: 2 },
    });
  });

  it('bounds prepared type roots by the existing request budget', async () => {
    const client = provider(
      Array.from({ length: 60 }, (_, index) => item(`root${index}`, index))
    );
    const result = await run('supertypes', client);
    expect(client.typeHierarchySupertypes).toHaveBeenCalledTimes(50);
    expect(result.payload).toMatchObject({
      completeness: {
        complete: false,
        preparedRootCount: 60,
        requestCount: 50,
        truncatedByBudget: true,
      },
    });
    expect(result.terminalLimit).toBe(true);
    expect(result.partialReasons).toContain('budget');
  });
});
