import { describe, it, expect } from 'vitest';
import {
  formatFileSize,
  parseFileSize,
} from '../../../octocode-tools-core/src/utils/file/size.js';

describe('fileSize utils', () => {
  describe('formatFileSize', () => {
    it('formats bytes to human readable strings', () => {
      expect(formatFileSize(0)).toBe('0.0B');
      expect(formatFileSize(512)).toBe('512.0B');
      expect(formatFileSize(1024)).toBe('1.0KB');
      expect(formatFileSize(1024 * 1024)).toBe('1.0MB');
      expect(formatFileSize(1024 * 1024 * 5)).toBe('5.0MB');
    });

    it('formats gigabytes', () => {
      expect(formatFileSize(1024 * 1024 * 1024)).toBe('1.0GB');
      expect(formatFileSize(5 * 1024 * 1024 * 1024)).toBe('5.0GB');
    });

    it('formats terabytes', () => {
      expect(formatFileSize(1024 * 1024 * 1024 * 1024)).toBe('1.0TB');
      expect(formatFileSize(2 * 1024 * 1024 * 1024 * 1024)).toBe('2.0TB');
    });
  });

  describe('parseFileSize', () => {
    it('parses human strings to bytes', () => {
      expect(parseFileSize('0')).toBe(0);
      expect(parseFileSize('512')).toBe(512);
      expect(parseFileSize('1K')).toBe(1024);
      expect(parseFileSize('1M')).toBe(1024 * 1024);
      expect(parseFileSize('5M')).toBe(5 * 1024 * 1024);
    });

    it('parses gigabytes and terabytes', () => {
      expect(parseFileSize('1G')).toBe(1024 * 1024 * 1024);
      expect(parseFileSize('2G')).toBe(2 * 1024 * 1024 * 1024);
      expect(parseFileSize('1T')).toBe(1024 * 1024 * 1024 * 1024);
    });

    it('parses full unit names (KB, MB, GB, TB, B)', () => {
      expect(parseFileSize('512B')).toBe(512);
      expect(parseFileSize('1.0KB')).toBe(1024);
      expect(parseFileSize('1.5MB')).toBe(Math.round(1.5 * 1024 * 1024));
      expect(parseFileSize('2.5GB')).toBe(Math.round(2.5 * 1024 * 1024 * 1024));
      expect(parseFileSize('1.0TB')).toBe(1024 * 1024 * 1024 * 1024);
    });

    it('handles case-insensitive units', () => {
      expect(parseFileSize('1kb')).toBe(1024);
      expect(parseFileSize('1mb')).toBe(1024 * 1024);
      expect(parseFileSize('1gb')).toBe(1024 * 1024 * 1024);
      expect(parseFileSize('1tb')).toBe(1024 * 1024 * 1024 * 1024);
      expect(parseFileSize('1b')).toBe(1);
    });

    it('throws error for invalid format', () => {
      expect(() => parseFileSize('invalid')).toThrow('Invalid size format');
      expect(() => parseFileSize('1X')).toThrow('Invalid size format');
      expect(() => parseFileSize('')).toThrow('Invalid size format');
    });
  });

  it('round-trips basic sizes with supported parser units', () => {
    const pairs: Array<[number, string]> = [
      [0, '0'],
      [512, '512'],
      [1024, '1K'],
      [1024 * 1024, '1M'],
    ];
    for (const [bytes, human] of pairs) {
      expect(parseFileSize(human)).toBe(bytes);
    }
  });
});
