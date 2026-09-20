import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock the interactive backend so no test renders a real inquirer prompt.
// Previously `selectWithCancel` was invoked for real here, which rendered a
// live selector to the terminal (ANSI/cursor-hide escapes, a dangling raw-mode
// stdin handle, and a hang/throw risk in non-TTY CI). The test "passed" only
// because the pending promise was abandoned. Now we drive it through a mock.
const { selectMock } = vi.hoisted(() => ({ selectMock: vi.fn() }));
vi.mock('@inquirer/prompts', () => ({
  select: selectMock,
  confirm: vi.fn(),
  input: vi.fn(),
  checkbox: vi.fn(),
  search: vi.fn(),
  Separator: class {
    type = 'separator' as const;
    separator: string;
    constructor(separator = '') {
      this.separator = separator;
    }
  },
}));

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
  beforeEach(() => {
    selectMock.mockReset();
  });

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
    const config = {
      message: 'Pick one',
      choices: [
        { name: 'Option A', value: 'a' },
        { name: 'Option B', value: 'b' },
      ],
    };

    it('forwards the config to the underlying select prompt', async () => {
      selectMock.mockResolvedValue('a');
      const result = await selectWithCancel(config);
      expect(selectMock).toHaveBeenCalledTimes(1);
      expect(selectMock).toHaveBeenCalledWith(config);
      expect(result).toBe('a');
    });

    it('propagates the underlying prompt rejection (e.g. user cancel)', async () => {
      const cancel = new Error('User force closed the prompt');
      selectMock.mockRejectedValue(cancel);
      await expect(selectWithCancel(config)).rejects.toBe(cancel);
    });
  });
});
