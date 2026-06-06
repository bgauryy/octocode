import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('../../src/lsp/validation.js', () => ({
  safeReadFile: vi.fn(),
}));

import { safeReadFile } from '../../src/lsp/validation.js';
import {
  enhanceIncomingCalls,
  enhanceOutgoingCalls,
  createCallItemKey,
} from '../../src/tools/lsp_call_hierarchy/callHierarchyHelpers.js';

const makeRange = (line: number) => ({
  start: { line, character: 0 },
  end: { line, character: 10 },
});

describe('callHierarchyHelpers - branch coverage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('enhanceIncomingCalls - null fileContent branch', () => {
    it('should keep original call when safeReadFile returns null', async () => {
      vi.mocked(safeReadFile).mockResolvedValue(null);
      const calls = [
        {
          from: {
            name: 'caller',
            kind: 'function' as const,
            uri: '/test/file.ts',
            range: makeRange(5),
            selectionRange: makeRange(5),
          },
          fromRanges: [makeRange(5)],
        },
      ];
      const result = await enhanceIncomingCalls(calls, 2);
      expect(result).toHaveLength(1);
      expect(result[0]!.from.name).toBe('caller');
      expect(result[0]!.from.content).toBeUndefined();
    });

    it('should keep original call when safeReadFile throws', async () => {
      vi.mocked(safeReadFile).mockRejectedValue(new Error('read failed'));
      const calls = [
        {
          from: {
            name: 'caller',
            kind: 'function' as const,
            uri: '/test/file.ts',
            range: makeRange(5),
            selectionRange: makeRange(5),
          },
          fromRanges: [makeRange(5)],
        },
      ];
      const result = await enhanceIncomingCalls(calls, 2);
      expect(result).toHaveLength(1);
      expect(result[0]!.from.name).toBe('caller');
    });
  });

  describe('enhanceOutgoingCalls - null fileContent branch', () => {
    it('should keep original call when safeReadFile returns null', async () => {
      vi.mocked(safeReadFile).mockResolvedValue(null);
      const calls = [
        {
          to: {
            name: 'callee',
            kind: 'function' as const,
            uri: '/test/target.ts',
            range: makeRange(10),
            selectionRange: makeRange(10),
          },
          fromRanges: [makeRange(3)],
        },
      ];
      const result = await enhanceOutgoingCalls(calls, 2);
      expect(result).toHaveLength(1);
      expect(result[0]!.to.name).toBe('callee');
      expect(result[0]!.to.content).toBeUndefined();
    });

    it('should keep original call when safeReadFile throws', async () => {
      vi.mocked(safeReadFile).mockRejectedValue(new Error('read failed'));
      const calls = [
        {
          to: {
            name: 'callee',
            kind: 'function' as const,
            uri: '/test/target.ts',
            range: makeRange(10),
            selectionRange: makeRange(10),
          },
          fromRanges: [makeRange(3)],
        },
      ];
      const result = await enhanceOutgoingCalls(calls, 2);
      expect(result).toHaveLength(1);
      expect(result[0]!.to.name).toBe('callee');
    });
  });

  describe('createCallItemKey', () => {
    it('should create unique key from uri, line, and name', () => {
      const item = {
        name: 'myFunc',
        kind: 'function' as const,
        uri: '/path/to/file.ts',
        range: {
          start: { line: 10, character: 0 },
          end: { line: 10, character: 6 },
        },
      };
      expect(createCallItemKey(item)).toBe('/path/to/file.ts:10:myFunc');
    });
  });
});
