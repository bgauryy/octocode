import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { c, bold, dim, underline } from '../src/utils/colors.js';

describe('Colors', () => {
  const originalIsTTY = process.stdout.isTTY;
  const originalNoColor = process.env.NO_COLOR;

  afterEach(() => {
    process.stdout.isTTY = originalIsTTY;
    if (originalNoColor === undefined) {
      delete process.env.NO_COLOR;
    } else {
      process.env.NO_COLOR = originalNoColor;
    }
  });

  describe('when output is an interactive TTY', () => {
    beforeEach(() => {
      process.stdout.isTTY = true;
      delete process.env.NO_COLOR;
    });

    it('should wrap text with color codes', () => {
      const result = c('red', 'hello');
      expect(result).toContain('\x1b[31m');
      expect(result).toContain('hello');
      expect(result).toContain('\x1b[0m');
    });

    it('should apply green color', () => {
      const result = c('green', 'success');
      expect(result).toContain('\x1b[32m');
      expect(result).toContain('success');
    });

    it('should apply cyan color', () => {
      const result = c('cyan', 'info');
      expect(result).toContain('\x1b[36m');
      expect(result).toContain('info');
    });

    it('should make text bold', () => {
      const result = bold('important');
      expect(result).toContain('\x1b[1m');
      expect(result).toContain('important');
    });

    it('should make text dim', () => {
      const result = dim('subtle');
      expect(result).toContain('\x1b[2m');
      expect(result).toContain('subtle');
    });

    it('should underline text', () => {
      const result = underline('link');
      expect(result).toContain('\x1b[4m');
      expect(result).toContain('link');
    });
  });

  describe('when output is not a TTY (piped / agent capture)', () => {
    beforeEach(() => {
      process.stdout.isTTY = false;
      delete process.env.NO_COLOR;
    });

    it('returns raw text with no ANSI escape codes', () => {
      expect(c('red', 'hello')).toBe('hello');
      expect(bold('important')).toBe('important');
      expect(dim('subtle')).toBe('subtle');
      expect(underline('link')).toBe('link');
    });
  });

  describe('when NO_COLOR is set', () => {
    beforeEach(() => {
      process.stdout.isTTY = true;
      process.env.NO_COLOR = '1';
    });

    it('returns raw text even on a TTY', () => {
      const result = c('red', 'hello');
      expect(result).toBe('hello');
      expect(result).not.toContain('\x1b');
    });
  });
});
