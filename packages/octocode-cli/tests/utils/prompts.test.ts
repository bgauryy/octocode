import { describe, it, expect } from 'vitest';
import {
  select,
  confirm,
  input,
  checkbox,
  search,
  Separator,
  loadInquirer,
  isInquirerLoaded,
  selectWithCancel,
} from '../../src/utils/prompts.js';

describe('Prompts Utilities', () => {
  describe('exports', () => {
    it('should export all prompt functions', () => {
      expect(typeof select).toBe('function');
      expect(typeof confirm).toBe('function');
      expect(typeof input).toBe('function');
      expect(typeof checkbox).toBe('function');
      expect(typeof search).toBe('function');
      expect(typeof selectWithCancel).toBe('function');
    });

    it('should export Separator class', () => {
      expect(Separator).toBeDefined();
      const sep = new Separator('---');
      expect(sep.type).toBe('separator');
    });
  });

  describe('isInquirerLoaded', () => {
    it('should always return true (statically imported)', () => {
      expect(isInquirerLoaded()).toBe(true);
    });
  });

  describe('loadInquirer', () => {
    it('should be a no-op that resolves', async () => {
      await expect(loadInquirer()).resolves.toBeUndefined();
    });

    it('should be safe to call multiple times', async () => {
      await loadInquirer();
      await loadInquirer();
      await loadInquirer();
      expect(isInquirerLoaded()).toBe(true);
    });
  });

  describe('selectWithCancel', () => {
    it('is a function that accepts a config object', () => {
      expect(typeof selectWithCancel).toBe('function');
      // Verify it accepts the same shape as select
      const config = {
        message: 'Pick one',
        choices: [
          { name: 'Option A', value: 'a' },
          { name: 'Option B', value: 'b' },
        ],
      };
      // selectWithCancel just calls through — we only verify it returns a Promise
      const result = selectWithCancel(config);
      expect(result).toBeInstanceOf(Promise);
      // Cancel the pending prompt to avoid test hang
      result.catch(() => {});
    });
  });
});
