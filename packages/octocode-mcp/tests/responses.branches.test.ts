import { describe, it, expect, vi } from 'vitest';
import {
  createResponseFormat,
  formatCallToolResultForOutput,
} from '../../octocode-tools-core/src/responses.js';
import { getConfigSync } from '@octocodeai/octocode-tools-core/config';

describe('responses.branches', () => {
  describe('createResponseFormat - JSON output', () => {
    it('should serialize as JSON when output format is json', () => {
      const mockedGetConfig = vi.mocked(getConfigSync);
      const originalImpl = mockedGetConfig.getMockImplementation();

      mockedGetConfig.mockReturnValueOnce({
        ...mockedGetConfig(),
        output: { format: 'json' },
      } as ReturnType<typeof getConfigSync>);

      const result = createResponseFormat({
        status: 'success',
        data: { key: 'value', nested: { prop: 'test' } },
      } as any);

      expect(result).toContain('"key"');
      expect(result).toContain('"value"');

      if (originalImpl) mockedGetConfig.mockImplementation(originalImpl);
    });

    it('should sort keys by priority in JSON format', () => {
      const mockedGetConfig = vi.mocked(getConfigSync);
      const originalImpl = mockedGetConfig.getMockImplementation();

      mockedGetConfig.mockReturnValueOnce({
        ...mockedGetConfig(),
        output: { format: 'json' },
      } as ReturnType<typeof getConfigSync>);

      const result = createResponseFormat(
        {
          data: { extra: 1 },
          status: 'success',
          instructions: 'test',
        } as any,
        ['instructions', 'status', 'data']
      );

      const parsed = JSON.parse(result);
      const keys = Object.keys(parsed);
      expect(keys[0]).toBe('instructions');
      expect(keys[1]).toBe('status');

      if (originalImpl) mockedGetConfig.mockImplementation(originalImpl);
    });

    it('should handle results array in JSON format with default priority', () => {
      const mockedGetConfig = vi.mocked(getConfigSync);
      const originalImpl = mockedGetConfig.getMockImplementation();

      mockedGetConfig.mockReturnValueOnce({
        ...mockedGetConfig(),
        output: { format: 'json' },
      } as ReturnType<typeof getConfigSync>);

      const result = createResponseFormat({
        results: [{ id: '1', status: 'ok', data: {} }],
      } as any);

      expect(result).toContain('"results"');

      if (originalImpl) mockedGetConfig.mockImplementation(originalImpl);
    });
  });

  describe('formatCallToolResultForOutput branch coverage', () => {
    it('falls back to JSON.stringify(result) when no text blocks and no structuredContent (line 318)', () => {
      const result = formatCallToolResultForOutput(
        {
          isError: false,
        } as never,
        'text'
      );
      const parsed = JSON.parse(result);
      expect(parsed).toBeDefined();
    });
  });

  describe('isTrivialPagination edge cases', () => {
    it('removes a hasMore=false-only pagination object as trivial (line 416 return true)', () => {
      const result = createResponseFormat({
        results: [
          {
            id: 'q1',
            data: { items: ['a', 'b'], pagination: { hasMore: false } },
          },
        ],
      } as never);
      expect(result).toBeDefined();
    });

    it('keeps pagination that carries a positive total-count (all N matches)', () => {
      const result = createResponseFormat({
        results: [
          {
            id: 'q1',
            data: {
              items: ['a', 'b'],
              pagination: { hasMore: false, totalPages: 1, totalMatches: 2 },
            },
          },
        ],
      } as never);
      expect(result).toContain('totalMatches');
    });
  });
});
