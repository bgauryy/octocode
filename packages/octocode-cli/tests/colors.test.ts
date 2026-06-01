import { describe, it, expect } from 'vitest';
import { c, bold, dim, underline } from '../src/utils/colors.js';

describe('Colors', () => {
  describe('c', () => {
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
  });

  describe('bold', () => {
    it('should make text bold', () => {
      const result = bold('important');
      expect(result).toContain('\x1b[1m');
      expect(result).toContain('important');
    });
  });

  describe('dim', () => {
    it('should make text dim', () => {
      const result = dim('subtle');
      expect(result).toContain('\x1b[2m');
      expect(result).toContain('subtle');
    });
  });

  describe('underline', () => {
    it('should underline text', () => {
      const result = underline('link');
      expect(result).toContain('\x1b[4m');
      expect(result).toContain('link');
    });
  });
});
