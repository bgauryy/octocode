import { describe, it, expect, vi, beforeEach, afterEach, Mock } from 'vitest';
import { registerLSPCallHierarchyTool } from '../../src/tools/lsp_call_hierarchy/register.js';
import { SymbolResolver } from '../../src/lsp/resolver.js';
import * as resolverModule from '../../src/lsp/resolver.js';
import * as managerModule from '../../src/lsp/manager.js';
import * as toolHelpers from '../../src/utils/file/toolHelpers.js';
import { safeExec } from '../../src/utils/exec/safe.js';
import { checkCommandAvailability } from '../../src/utils/exec/commandAvailability.js';
import * as fsPromises from 'fs/promises';

vi.mock('fs/promises', () => ({
  readFile: vi.fn(),
}));

vi.mock('../../src/lsp/resolver.js', () => ({
  SymbolResolver: vi.fn(),
  SymbolResolutionError: class extends Error {},
}));

vi.mock('../../src/lsp/manager.js', () => ({
  LSP_UNAVAILABLE_HINT: 'LSP unavailable test',
  acquirePooledClient: vi.fn(),
  isLanguageServerAvailable: vi.fn(),
}));

vi.mock('../../src/utils/file/toolHelpers.js', () => ({
  validateToolPath: vi.fn(),
  createErrorResult: vi.fn(error => ({
    isError: true,
    error: error?.message || String(error),
    status: 'error',
  })),
}));

vi.mock('../../src/utils/exec/safe.js', () => ({
  safeExec: vi.fn(),
}));

vi.mock('../../src/utils/exec/commandAvailability.js', () => ({
  checkCommandAvailability: vi.fn(),
}));

vi.mock('../../src/hints/index.js', () => {
  return {
    getHints: vi.fn(() => []),
  };
});

vi.mock('../../src/utils/response/bulk.js', () => ({
  executeBulkOperation: vi.fn(async (queries, handler) => {
    const results = [];
    for (const query of queries) {
      results.push(await handler(query));
    }
    return { content: [{ type: 'text', text: JSON.stringify(results) }] };
  }),
}));

describe('LSP Call Hierarchy Coverage Tests', () => {
  let toolHandler: any;
  let mockServer: any;
  let mockSymbolResolver: any;
  let mockLSPClient: any;

  beforeEach(async () => {
    vi.clearAllMocks();

    mockServer = {
      registerTool: vi.fn((name, schema, handler) => {
        toolHandler = handler;
        return { name, schema, handler };
      }),
    };

    mockSymbolResolver = {
      resolvePositionFromContent: vi.fn(),
    };
    (SymbolResolver as Mock).mockImplementation(function () {
      return mockSymbolResolver;
    });

    mockLSPClient = {
      stop: vi.fn(),
      prepareCallHierarchy: vi.fn(),
      getIncomingCalls: vi.fn(),
      getOutgoingCalls: vi.fn(),
    };
    (managerModule.acquirePooledClient as Mock).mockResolvedValue(
      mockLSPClient
    );

    registerLSPCallHierarchyTool(mockServer);
  });

  afterEach(() => {
    vi.resetAllMocks();
  });

  describe('Execution Flow', () => {
    const baseQuery = {
      uri: '/workspace/file.ts',
      symbolName: 'myFunc',
      lineHint: 10,
      direction: 'incoming',
      researchGoal: 'goal',
      reasoning: 'reason',
      mainResearchGoal: 'main',
    };

    it('should handle invalid path', async () => {
      (toolHelpers.validateToolPath as Mock).mockReturnValue({
        isValid: false,
        errorResult: { isError: true, message: 'Invalid path' },
      });

      const result = await toolHandler({ queries: [baseQuery] });
      const results = JSON.parse(result.content[0].text);
      expect(results[0]).toMatchObject({
        isError: true,
        message: 'Invalid path',
      });
    });

    it('should handle file read error', async () => {
      (toolHelpers.validateToolPath as Mock).mockReturnValue({
        isValid: true,
        sanitizedPath: '/workspace/file.ts',
      });
      (fsPromises.readFile as Mock).mockRejectedValue(
        new Error('Access denied')
      );
      (toolHelpers.createErrorResult as Mock).mockReturnValue({
        error: 'Access denied',
      });

      const result = await toolHandler({ queries: [baseQuery] });
      const results = JSON.parse(result.content[0].text);
      expect(toolHelpers.createErrorResult).toHaveBeenCalled();
      expect(results[0]).toMatchObject({ error: 'Access denied' });
    });

    it('should handle symbol resolution error', async () => {
      (toolHelpers.validateToolPath as Mock).mockReturnValue({
        isValid: true,
        sanitizedPath: '/workspace/file.ts',
      });
      (fsPromises.readFile as Mock).mockResolvedValue('content');

      mockSymbolResolver.resolvePositionFromContent.mockImplementation(() => {
        throw new resolverModule.SymbolResolutionError(
          'myFunc',
          10,
          'Symbol not found'
        );
      });

      const result = await toolHandler({ queries: [baseQuery] });
      const results = JSON.parse(result.content[0].text);
      expect(results[0].status).toBe('empty');
      expect(results[0].errorType).toBe('symbol_not_found');
    });

    describe('LSP Path', () => {
      beforeEach(() => {
        (toolHelpers.validateToolPath as Mock).mockReturnValue({
          isValid: true,
          sanitizedPath: '/workspace/file.ts',
        });
        (fsPromises.readFile as Mock).mockResolvedValue('function myFunc() {}');
        mockSymbolResolver.resolvePositionFromContent.mockReturnValue({
          position: { line: 0, character: 0 },
          foundAtLine: 1,
        });
        (managerModule.isLanguageServerAvailable as Mock).mockResolvedValue(
          true
        );
      });

      it('should use LSP incoming calls when available', async () => {
        mockLSPClient.prepareCallHierarchy.mockResolvedValue([
          {
            name: 'myFunc',
            uri: 'file:///workspace/file.ts',
            range: {
              start: { line: 0, character: 0 },
              end: { line: 0, character: 10 },
            },
            selectionRange: {
              start: { line: 0, character: 0 },
              end: { line: 0, character: 10 },
            },
          },
        ]);

        mockLSPClient.getIncomingCalls.mockResolvedValue([
          {
            from: {
              name: 'caller',
              uri: 'file:///workspace/caller.ts',
              range: {
                start: { line: 0, character: 0 },
                end: { line: 0, character: 10 },
              },
              selectionRange: {
                start: { line: 0, character: 0 },
                end: { line: 0, character: 10 },
              },
            },
            fromRanges: [
              {
                start: { line: 0, character: 0 },
                end: { line: 0, character: 10 },
              },
            ],
          },
        ]);

        const result = await toolHandler({ queries: [baseQuery] });
        const results = JSON.parse(result.content[0].text);

        expect(results[0].status).toBeUndefined();
        expect(results[0].incomingCalls).toHaveLength(1);
        expect(results[0].incomingCalls[0].from.name).toBe('caller');
      });

      it('should handle empty incoming calls from LSP', async () => {
        mockLSPClient.prepareCallHierarchy.mockResolvedValue([
          {
            name: 'myFunc',
            uri: 'file:///workspace/file.ts',
            range: {
              start: { line: 0, character: 0 },
              end: { line: 0, character: 10 },
            },
            selectionRange: {
              start: { line: 0, character: 0 },
              end: { line: 0, character: 10 },
            },
          },
        ]);
        mockLSPClient.getIncomingCalls.mockResolvedValue([]);

        const result = await toolHandler({ queries: [baseQuery] });
        const results = JSON.parse(result.content[0].text);

        expect(results[0].status).toBe('empty');
        expect(results[0].incomingCalls).toEqual([]);
      });

      it('should use LSP outgoing calls when available', async () => {
        const outgoingQuery = { ...baseQuery, direction: 'outgoing' };

        mockLSPClient.prepareCallHierarchy.mockResolvedValue([
          {
            name: 'myFunc',
            uri: 'file:///workspace/file.ts',
            range: {
              start: { line: 0, character: 0 },
              end: { line: 0, character: 10 },
            },
            selectionRange: {
              start: { line: 0, character: 0 },
              end: { line: 0, character: 10 },
            },
          },
        ]);
        mockLSPClient.getOutgoingCalls.mockResolvedValue([
          {
            to: {
              name: 'callee',
              uri: 'file:///workspace/callee.ts',
              range: {
                start: { line: 0, character: 0 },
                end: { line: 0, character: 10 },
              },
              selectionRange: {
                start: { line: 0, character: 0 },
                end: { line: 0, character: 10 },
              },
            },
            fromRanges: [],
          },
        ]);

        const result = await toolHandler({ queries: [outgoingQuery] });
        const results = JSON.parse(result.content[0].text);

        expect(results[0].status).toBeUndefined();
        expect(results[0].outgoingCalls).toHaveLength(1);
        expect(results[0].outgoingCalls[0].to.name).toBe('callee');
      });

      it('should return empty status when LSP outgoing calls returns empty (line 166)', async () => {
        const outgoingQuery = { ...baseQuery, direction: 'outgoing' };

        mockLSPClient.prepareCallHierarchy.mockResolvedValue([
          {
            name: 'myFunc',
            uri: 'file:///workspace/file.ts',
            range: {
              start: { line: 0, character: 0 },
              end: { line: 0, character: 10 },
            },
            selectionRange: {
              start: { line: 0, character: 0 },
              end: { line: 0, character: 10 },
            },
          },
        ]);
        mockLSPClient.getOutgoingCalls.mockResolvedValue([]);

        const result = await toolHandler({ queries: [outgoingQuery] });
        const results = JSON.parse(result.content[0].text);

        expect(results[0].status).toBe('empty');
        expect(results[0].outgoingCalls).toEqual([]);
        expect(results[0].direction).toBe('outgoing');
      });

      it('should handle LSP prepareCallHierarchy returning empty', async () => {
        mockLSPClient.prepareCallHierarchy.mockResolvedValue([]);

        const result = await toolHandler({ queries: [baseQuery] });
        const results = JSON.parse(result.content[0].text);

        expect(results[0].status).toBe('empty');
        expect(results[0].error).toContain('No callable symbol found');
      });

      it('should return empty (LSP_EMPTY) when available LSP throws', async () => {
        mockLSPClient.prepareCallHierarchy.mockRejectedValue(
          new Error('LSP error')
        );

        const result = await toolHandler({ queries: [baseQuery] });
        const results = JSON.parse(result.content[0].text);

        expect(results[0].status).toBe('empty');
        expect(results[0].errorCode).toBe('LSP_EMPTY');
        expect(results[0].incomingCalls).toBeUndefined();
        expect(results[0].hints.length).toBeGreaterThan(0);
      });
    });

    describe('LSP Unavailable', () => {
      beforeEach(() => {
        (toolHelpers.validateToolPath as Mock).mockReturnValue({
          isValid: true,
          sanitizedPath: '/workspace/file.ts',
        });
        (fsPromises.readFile as Mock).mockResolvedValue('function myFunc() {}');
        mockSymbolResolver.resolvePositionFromContent.mockReturnValue({
          position: { line: 0, character: 0 },
          foundAtLine: 1,
        });
        (managerModule.isLanguageServerAvailable as Mock).mockResolvedValue(
          false
        );
      });

      it('returns empty LSP_NOT_INSTALLED with no call graph (incoming)', async () => {
        const result = await toolHandler({ queries: [baseQuery] });
        const results = JSON.parse(result.content[0].text);

        expect(results[0].status).toBe('empty');
        expect(results[0].errorCode).toBe('LSP_NOT_INSTALLED');
        expect(results[0].direction).toBe('incoming');
        expect(results[0].incomingCalls).toBeUndefined();
        expect(results[0].hints.length).toBeGreaterThan(0);
      });

      it('returns empty LSP_NOT_INSTALLED with no call graph (outgoing)', async () => {
        const outgoingQuery = { ...baseQuery, direction: 'outgoing' };

        const result = await toolHandler({ queries: [outgoingQuery] });
        const results = JSON.parse(result.content[0].text);

        expect(results[0].status).toBe('empty');
        expect(results[0].errorCode).toBe('LSP_NOT_INSTALLED');
        expect(results[0].direction).toBe('outgoing');
        expect(results[0].outgoingCalls).toBeUndefined();
      });

      it('does not invoke ripgrep/grep when LSP is unavailable', async () => {
        await toolHandler({ queries: [baseQuery] });

        expect(safeExec).not.toHaveBeenCalled();
      });
    });
  });
});
