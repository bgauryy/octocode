import { describe, it, expect } from 'vitest';
import { validateToolCallBody, getValidationHints, MAX_QUERIES } from '../../validation/toolCallSchema.js';
import type { z } from 'zod';

describe('validateToolCallBody', () => {
  describe('valid inputs', () => {
    it('accepts single query', () => {
      const result = validateToolCallBody({
        queries: [{ pattern: 'test', path: '/tmp' }],
      });

      expect(result.success).toBe(true);
      expect(result.data?.queries).toHaveLength(1);
    });

    it('accepts multiple queries up to MAX_QUERIES', () => {
      const queries = Array.from({ length: MAX_QUERIES }, (_, i) => ({
        pattern: `test${i}`,
        path: '/tmp',
      }));

      const result = validateToolCallBody({ queries });

      expect(result.success).toBe(true);
      expect(result.data?.queries).toHaveLength(MAX_QUERIES);
    });

    it('accepts queries with arbitrary fields', () => {
      const result = validateToolCallBody({
        queries: [
          {
            pattern: 'test',
            path: '/tmp',
            mainResearchGoal: 'Testing',
            customField: 123,
          },
        ],
      });

      expect(result.success).toBe(true);
    });
  });

  describe('invalid inputs', () => {
    it('rejects missing queries field', () => {
      const result = validateToolCallBody({});

      expect(result.success).toBe(false);
      expect(result.error?.message).toMatch(/Required|expected array/i);
    });

    it('rejects null queries', () => {
      const result = validateToolCallBody({ queries: null });

      expect(result.success).toBe(false);
    });

    it('rejects empty queries array', () => {
      const result = validateToolCallBody({ queries: [] });

      expect(result.success).toBe(false);
      expect(result.error?.message).toContain('At least one query');
    });

    it('rejects too many queries', () => {
      const queries = Array.from({ length: MAX_QUERIES + 1 }, (_, i) => ({
        pattern: `test${i}`,
      }));

      const result = validateToolCallBody({ queries });

      expect(result.success).toBe(false);
      expect(result.error?.message).toContain(`Maximum ${MAX_QUERIES}`);
    });

    it('rejects empty query objects', () => {
      const result = validateToolCallBody({ queries: [{}] });

      expect(result.success).toBe(false);
      expect(result.error?.message).toContain('cannot be empty');
    });

    it('rejects non-array queries', () => {
      const result = validateToolCallBody({ queries: 'not an array' });

      expect(result.success).toBe(false);
    });
  });
});

describe('getValidationHints', () => {
  it('includes primary error message', () => {
    const hints = getValidationHints('localSearchCode', {
      message: 'Missing queries',
      details: [],
    });

    expect(hints).toContain('Missing queries');
  });

  it('includes tool schema hint', () => {
    const hints = getValidationHints('localSearchCode', {
      message: 'Error',
      details: [],
    });

    expect(hints.some((h) => h.includes('/tools/info/localSearchCode'))).toBe(true);
  });

  it('includes queries format hint for queries errors', () => {
    const hints = getValidationHints('localSearchCode', {
      message: 'queries is required',
      details: [{ path: ['queries'], message: 'Required', code: 'invalid_type', expected: 'array' } as z.core.$ZodIssue],
    });

    expect(hints.some((h) => h.includes('{ "queries": [{ ... }] }'))).toBe(true);
  });
});

describe('MAX_QUERIES constant', () => {
  it('is set to 3', () => {
    expect(MAX_QUERIES).toBe(3);
  });
});
