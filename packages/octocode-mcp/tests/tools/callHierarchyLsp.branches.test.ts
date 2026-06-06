import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { LSPCallHierarchyQuery } from '@octocodeai/octocode-core';
import {
  callHierarchyWithLSP,
  gatherIncomingCallsRecursive,
  gatherOutgoingCallsRecursive,
} from '../../src/tools/lsp_call_hierarchy/callHierarchyLsp.js';
import { createCallItemKey } from '../../src/tools/lsp_call_hierarchy/callHierarchyHelpers.js';

vi.mock('node:fs/promises', () => ({
  readFile: vi.fn().mockResolvedValue('export function testFunction() {}'),
}));

vi.mock('../../src/lsp/manager.js', () => ({
  LSP_UNAVAILABLE_HINT: 'LSP unavailable test',
  acquirePooledClient: vi.fn(),
}));

vi.mock('../../src/tools/lsp_call_hierarchy/callHierarchyHelpers.js', () => ({
  createCallItemKey: vi.fn(
    item => `${item.uri}:${item.range.start.line}:${item.name}`
  ),
  enhanceCallHierarchyItem: vi.fn(async item => ({
    ...item,
    content: `>    ${item.range.start.line + 1}| ${item.name}()`,
    displayRange: {
      startLine: item.range.start.line + 1,
      endLine: item.range.end.line + 1,
    },
  })),
  enhanceIncomingCalls: vi.fn(async calls => calls),
  enhanceOutgoingCalls: vi.fn(async calls => calls),
  paginateResults: vi.fn((items, perPage, page) => ({
    paginatedItems: items.slice((page - 1) * perPage, page * perPage),
    pagination: {
      currentPage: page,
      totalPages: Math.ceil(items.length / perPage),
      totalResults: items.length,
      hasMore: page < Math.ceil(items.length / perPage),
      resultsPerPage: perPage,
    },
  })),
}));

import * as lspModule from '../../src/lsp/manager.js';

describe('LSP Call Hierarchy - Branch Coverage Tests', () => {
  const baseQuery: LSPCallHierarchyQuery = {
    uri: '/workspace/src/file.ts',
    symbolName: 'testFunction',
    lineHint: 5,
    direction: 'incoming',
    researchGoal: 'test',
    reasoning: 'test',
    page: 1,
    depth: 1,
    contextLines: 2,
    orderHint: 0,
    callsPerPage: 15,
  } as any;

  const mockClient = {
    hasCapability: vi.fn(),
    prepareCallHierarchy: vi.fn(),
    getIncomingCalls: vi.fn(),
    getOutgoingCalls: vi.fn(),
    gotoDefinition: vi.fn(),
    stop: vi.fn().mockResolvedValue(undefined),
  };

  const mockCallHierarchyItem = {
    name: 'testFunction',
    kind: 'function' as const,
    uri: '/workspace/src/file.ts',
    range: {
      start: { line: 4, character: 0 },
      end: { line: 4, character: 20 },
    },
    selectionRange: {
      start: { line: 4, character: 0 },
      end: { line: 4, character: 20 },
    },
  };

  beforeEach(() => {
    vi.clearAllMocks();
    process.env.WORKSPACE_ROOT = '/workspace';

    vi.mocked(lspModule.acquirePooledClient).mockResolvedValue(
      mockClient as any
    );
    vi.mocked(mockClient.hasCapability).mockReturnValue(true);
    vi.mocked(mockClient.prepareCallHierarchy).mockResolvedValue([
      mockCallHierarchyItem,
    ]);
    vi.mocked(mockClient.getIncomingCalls).mockResolvedValue([]);
    vi.mocked(mockClient.getOutgoingCalls).mockResolvedValue([]);
    vi.mocked(mockClient.gotoDefinition).mockResolvedValue([]);
  });

  describe('callHierarchyWithLSP - Direction and Client Branches', () => {
    it('returns structured error when call hierarchy capability is unsupported', async () => {
      vi.mocked(mockClient.hasCapability).mockReturnValue(false);

      const result = await callHierarchyWithLSP(
        '/workspace/src/file.ts',
        '/workspace',
        { line: 4, character: 0 },
        { ...baseQuery, direction: 'incoming' },
        'export function testFunction() {}'
      );

      expect(result).toMatchObject({
        status: 'error',
        errorCode: 'LSP_CAPABILITY_UNSUPPORTED',
        direction: 'incoming',
        depth: 1,
      });
      expect((result as Record<string, unknown>).lspMode).toBeUndefined();
      expect(mockClient.prepareCallHierarchy).not.toHaveBeenCalled();
    });

    it('should handle incoming direction path', async () => {
      const incomingCall = {
        from: {
          ...mockCallHierarchyItem,
          name: 'callerFunction',
          uri: '/workspace/src/caller.ts',
          range: {
            start: { line: 10, character: 0 },
            end: { line: 10, character: 20 },
          },
        },
        fromRanges: [
          {
            start: { line: 10, character: 0 },
            end: { line: 10, character: 20 },
          },
        ],
      };
      vi.mocked(mockClient.getIncomingCalls).mockResolvedValue([incomingCall]);

      const result = await callHierarchyWithLSP(
        '/workspace/src/file.ts',
        '/workspace',
        { line: 4, character: 0 },
        { ...baseQuery, direction: 'incoming' },
        'export function testFunction() {}'
      );

      expect(result).not.toBeNull();
      expect(result?.direction).toBe('incoming');
      expect(result?.status).toBeUndefined();
      expect(mockClient.getIncomingCalls).toHaveBeenCalled();
    });

    it('should handle outgoing direction path', async () => {
      const outgoingCall = {
        to: {
          ...mockCallHierarchyItem,
          name: 'calleeFunction',
          uri: '/workspace/src/callee.ts',
          range: {
            start: { line: 15, character: 0 },
            end: { line: 15, character: 20 },
          },
        },
        fromRanges: [
          { start: { line: 4, character: 0 }, end: { line: 4, character: 20 } },
        ],
      };
      vi.mocked(mockClient.getOutgoingCalls).mockResolvedValue([outgoingCall]);

      const result = await callHierarchyWithLSP(
        '/workspace/src/file.ts',
        '/workspace',
        { line: 4, character: 0 },
        { ...baseQuery, direction: 'outgoing' },
        'export function testFunction() {}'
      );

      expect(result).not.toBeNull();
      expect(result?.direction).toBe('outgoing');
      expect(result?.status).toBeUndefined();
      expect(mockClient.getOutgoingCalls).toHaveBeenCalled();
    });

    it('should return null when no client available', async () => {
      vi.mocked(lspModule.acquirePooledClient).mockResolvedValue(null);

      const result = await callHierarchyWithLSP(
        '/workspace/src/file.ts',
        '/workspace',
        { line: 4, character: 0 },
        baseQuery,
        'export function testFunction() {}'
      );

      expect(result).toBeNull();
    });
  });

  describe('gatherIncomingCallsRecursive - Branch Coverage', () => {
    it('should return empty array when remainingDepth <= 0', async () => {
      const result = await gatherIncomingCallsRecursive(
        mockClient as any,
        mockCallHierarchyItem,
        0,
        new Set(),
        2
      );

      expect(result).toEqual([]);
      expect(mockClient.getIncomingCalls).not.toHaveBeenCalled();
    });

    it('should return empty array when client is null', async () => {
      const result = await gatherIncomingCallsRecursive(
        null as any,
        mockCallHierarchyItem,
        2,
        new Set(),
        2
      );

      expect(result).toEqual([]);
    });

    it('should return direct calls when remainingDepth === 1 (no recursion)', async () => {
      const incomingCall = {
        from: {
          ...mockCallHierarchyItem,
          name: 'callerFunction',
          uri: '/workspace/src/caller.ts',
          range: {
            start: { line: 10, character: 0 },
            end: { line: 10, character: 20 },
          },
        },
        fromRanges: [
          {
            start: { line: 10, character: 0 },
            end: { line: 10, character: 20 },
          },
        ],
      };
      vi.mocked(mockClient.getIncomingCalls).mockResolvedValue([incomingCall]);

      const result = await gatherIncomingCallsRecursive(
        mockClient as any,
        mockCallHierarchyItem,
        1,
        new Set(),
        2
      );

      expect(result).toHaveLength(1);
      expect(mockClient.getIncomingCalls).toHaveBeenCalledTimes(1);
    });

    it('should skip cycles when visited.has(key) (depth > 1)', async () => {
      const incomingCall = {
        from: {
          ...mockCallHierarchyItem,
          name: 'callerFunction',
          uri: '/workspace/src/caller.ts',
          range: {
            start: { line: 10, character: 0 },
            end: { line: 10, character: 20 },
          },
        },
        fromRanges: [
          {
            start: { line: 10, character: 0 },
            end: { line: 10, character: 20 },
          },
        ],
      };
      vi.mocked(mockClient.getIncomingCalls).mockResolvedValue([incomingCall]);

      const visited = new Set<string>();
      const key = createCallItemKey(incomingCall.from);
      visited.add(key);

      const result = await gatherIncomingCallsRecursive(
        mockClient as any,
        mockCallHierarchyItem,
        2,
        visited,
        2
      );

      expect(result).toHaveLength(1);
      expect(mockClient.getIncomingCalls).toHaveBeenCalledTimes(1);
    });

    it('should recursively gather calls when depth > 1', async () => {
      const incomingCall1 = {
        from: {
          ...mockCallHierarchyItem,
          name: 'callerFunction1',
          uri: '/workspace/src/caller1.ts',
          range: {
            start: { line: 10, character: 0 },
            end: { line: 10, character: 20 },
          },
        },
        fromRanges: [
          {
            start: { line: 10, character: 0 },
            end: { line: 10, character: 20 },
          },
        ],
      };

      const incomingCall2 = {
        from: {
          ...mockCallHierarchyItem,
          name: 'callerFunction2',
          uri: '/workspace/src/caller2.ts',
          range: {
            start: { line: 20, character: 0 },
            end: { line: 20, character: 20 },
          },
        },
        fromRanges: [
          {
            start: { line: 20, character: 0 },
            end: { line: 20, character: 20 },
          },
        ],
      };

      vi.mocked(mockClient.getIncomingCalls)
        .mockResolvedValueOnce([incomingCall1])
        .mockResolvedValueOnce([incomingCall2]);

      const visited = new Set<string>();
      const result = await gatherIncomingCallsRecursive(
        mockClient as any,
        mockCallHierarchyItem,
        2,
        visited,
        2
      );

      expect(result).toHaveLength(2);
      expect(mockClient.getIncomingCalls).toHaveBeenCalledTimes(2);
    });

    it('should skip enhancement when contextLines is 0', async () => {
      const { enhanceIncomingCalls } =
        await import('../../src/tools/lsp_call_hierarchy/callHierarchyHelpers.js');
      const incomingCall = {
        from: {
          ...mockCallHierarchyItem,
          name: 'callerFunction',
          uri: '/workspace/src/caller.ts',
          range: {
            start: { line: 10, character: 0 },
            end: { line: 10, character: 20 },
          },
        },
        fromRanges: [
          {
            start: { line: 10, character: 0 },
            end: { line: 10, character: 20 },
          },
        ],
      };
      vi.mocked(mockClient.getIncomingCalls).mockResolvedValue([incomingCall]);

      const result = await gatherIncomingCallsRecursive(
        mockClient as any,
        mockCallHierarchyItem,
        1,
        new Set(),
        0
      );

      expect(result).toHaveLength(1);
      expect(enhanceIncomingCalls).not.toHaveBeenCalled();
    });
  });

  describe('gatherOutgoingCallsRecursive - Branch Coverage', () => {
    it('should return empty array when remainingDepth <= 0', async () => {
      const result = await gatherOutgoingCallsRecursive(
        mockClient as any,
        mockCallHierarchyItem,
        0,
        new Set(),
        2
      );

      expect(result).toEqual([]);
      expect(mockClient.getOutgoingCalls).not.toHaveBeenCalled();
    });

    it('should return empty array when client is null', async () => {
      const result = await gatherOutgoingCallsRecursive(
        null as any,
        mockCallHierarchyItem,
        2,
        new Set(),
        2
      );

      expect(result).toEqual([]);
    });

    it('should return direct calls when remainingDepth === 1 (no recursion)', async () => {
      const outgoingCall = {
        to: {
          ...mockCallHierarchyItem,
          name: 'calleeFunction',
          uri: '/workspace/src/callee.ts',
          range: {
            start: { line: 15, character: 0 },
            end: { line: 15, character: 20 },
          },
        },
        fromRanges: [
          { start: { line: 4, character: 0 }, end: { line: 4, character: 20 } },
        ],
      };
      vi.mocked(mockClient.getOutgoingCalls).mockResolvedValue([outgoingCall]);

      const result = await gatherOutgoingCallsRecursive(
        mockClient as any,
        mockCallHierarchyItem,
        1,
        new Set(),
        2
      );

      expect(result).toHaveLength(1);
      expect(mockClient.getOutgoingCalls).toHaveBeenCalledTimes(1);
    });

    it('should skip cycles when visited.has(key) (depth > 1)', async () => {
      const outgoingCall = {
        to: {
          ...mockCallHierarchyItem,
          name: 'calleeFunction',
          uri: '/workspace/src/callee.ts',
          range: {
            start: { line: 15, character: 0 },
            end: { line: 15, character: 20 },
          },
        },
        fromRanges: [
          { start: { line: 4, character: 0 }, end: { line: 4, character: 20 } },
        ],
      };
      vi.mocked(mockClient.getOutgoingCalls).mockResolvedValue([outgoingCall]);

      const visited = new Set<string>();
      const key = createCallItemKey(outgoingCall.to);
      visited.add(key);

      const result = await gatherOutgoingCallsRecursive(
        mockClient as any,
        mockCallHierarchyItem,
        2,
        visited,
        2
      );

      expect(result).toHaveLength(1);
      expect(mockClient.getOutgoingCalls).toHaveBeenCalledTimes(1);
    });

    it('should recursively gather calls when depth > 1', async () => {
      const outgoingCall1 = {
        to: {
          ...mockCallHierarchyItem,
          name: 'calleeFunction1',
          uri: '/workspace/src/callee1.ts',
          range: {
            start: { line: 15, character: 0 },
            end: { line: 15, character: 20 },
          },
        },
        fromRanges: [
          { start: { line: 4, character: 0 }, end: { line: 4, character: 20 } },
        ],
      };

      const outgoingCall2 = {
        to: {
          ...mockCallHierarchyItem,
          name: 'calleeFunction2',
          uri: '/workspace/src/callee2.ts',
          range: {
            start: { line: 25, character: 0 },
            end: { line: 25, character: 20 },
          },
        },
        fromRanges: [
          {
            start: { line: 15, character: 0 },
            end: { line: 15, character: 20 },
          },
        ],
      };

      vi.mocked(mockClient.getOutgoingCalls)
        .mockResolvedValueOnce([outgoingCall1])
        .mockResolvedValueOnce([outgoingCall2]);

      const visited = new Set<string>();
      const result = await gatherOutgoingCallsRecursive(
        mockClient as any,
        mockCallHierarchyItem,
        2,
        visited,
        2
      );

      expect(result).toHaveLength(2);
      expect(mockClient.getOutgoingCalls).toHaveBeenCalledTimes(2);
    });

    it('should skip enhancement when contextLines is 0', async () => {
      const { enhanceOutgoingCalls } =
        await import('../../src/tools/lsp_call_hierarchy/callHierarchyHelpers.js');
      const outgoingCall = {
        to: {
          ...mockCallHierarchyItem,
          name: 'calleeFunction',
          uri: '/workspace/src/callee.ts',
          range: {
            start: { line: 15, character: 0 },
            end: { line: 15, character: 20 },
          },
        },
        fromRanges: [
          { start: { line: 4, character: 0 }, end: { line: 4, character: 20 } },
        ],
      };
      vi.mocked(mockClient.getOutgoingCalls).mockResolvedValue([outgoingCall]);

      const result = await gatherOutgoingCallsRecursive(
        mockClient as any,
        mockCallHierarchyItem,
        1,
        new Set(),
        0
      );

      expect(result).toHaveLength(1);
      expect(enhanceOutgoingCalls).not.toHaveBeenCalled();
    });
  });

  describe('Auto-follow from import lines', () => {
    const definitionItem = {
      name: 'testFunction',
      kind: 'function' as const,
      uri: '/workspace/src/definitions.ts',
      range: {
        start: { line: 10, character: 0 },
        end: { line: 10, character: 30 },
      },
      selectionRange: {
        start: { line: 10, character: 0 },
        end: { line: 10, character: 30 },
      },
    };

    it('should auto-follow to definition when prepareCallHierarchy returns empty', async () => {
      vi.mocked(mockClient.prepareCallHierarchy)
        .mockResolvedValueOnce([])
        .mockResolvedValueOnce([definitionItem]);

      vi.mocked(mockClient.gotoDefinition).mockResolvedValue([
        {
          uri: '/workspace/src/definitions.ts',
          range: {
            start: { line: 10, character: 0 },
            end: { line: 10, character: 30 },
          },
          content: 'export function testFunction() {}',
        },
      ]);

      const incomingCall = {
        from: {
          ...mockCallHierarchyItem,
          name: 'callerFunction',
          uri: '/workspace/src/caller.ts',
          range: {
            start: { line: 5, character: 0 },
            end: { line: 5, character: 20 },
          },
        },
        fromRanges: [
          {
            start: { line: 5, character: 0 },
            end: { line: 5, character: 20 },
          },
        ],
      };
      vi.mocked(mockClient.getIncomingCalls).mockResolvedValue([incomingCall]);

      const result = await callHierarchyWithLSP(
        '/workspace/src/file.ts',
        '/workspace',
        { line: 1, character: 10 },
        { ...baseQuery, direction: 'incoming' },
        'import { testFunction } from "./definitions";'
      );

      expect(result).not.toBeNull();
      expect(result?.status).toBeUndefined();
      expect(mockClient.gotoDefinition).toHaveBeenCalledWith(
        '/workspace/src/file.ts',
        { line: 1, character: 10 }
      );
      expect(mockClient.prepareCallHierarchy).toHaveBeenCalledTimes(2);
    });

    it('should return empty when auto-follow also fails', async () => {
      vi.mocked(mockClient.prepareCallHierarchy).mockResolvedValue([]);
      vi.mocked(mockClient.gotoDefinition).mockResolvedValue([]);

      const result = await callHierarchyWithLSP(
        '/workspace/src/file.ts',
        '/workspace',
        { line: 1, character: 10 },
        { ...baseQuery, direction: 'incoming' },
        'import { testFunction } from "./definitions";'
      );

      expect(result).not.toBeNull();
      expect(result?.status).toBe('empty');
      expect(result?.errorType).toBe('symbol_not_found');
    });

    it('should return empty when gotoDefinition returns no range', async () => {
      vi.mocked(mockClient.prepareCallHierarchy).mockResolvedValue([]);
      vi.mocked(mockClient.gotoDefinition).mockResolvedValue([
        { uri: '', range: null as any, content: '' },
      ]);

      const result = await callHierarchyWithLSP(
        '/workspace/src/file.ts',
        '/workspace',
        { line: 1, character: 10 },
        { ...baseQuery, direction: 'incoming' },
        'import { testFunction } from "./definitions";'
      );

      expect(result?.status).toBe('empty');
    });

    it('should handle gotoDefinition throwing an error gracefully', async () => {
      vi.mocked(mockClient.prepareCallHierarchy).mockResolvedValue([]);
      vi.mocked(mockClient.gotoDefinition).mockRejectedValue(
        new Error('LSP error')
      );

      const result = await callHierarchyWithLSP(
        '/workspace/src/file.ts',
        '/workspace',
        { line: 1, character: 10 },
        { ...baseQuery, direction: 'incoming' },
        'import { testFunction } from "./definitions";'
      );

      expect(result?.status).toBe('empty');
      expect(result?.errorType).toBe('symbol_not_found');
    });
  });
});
