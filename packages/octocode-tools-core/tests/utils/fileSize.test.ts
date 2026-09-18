import { describe, expect, it } from 'vitest';
import {
  formatFileSize,
  parseFileSize,
} from '../../src/utils/file/size.js';

describe('file size helpers', () => {
  it.each([
    [0, '0.0B'],
    [17, '17.0B'],
    [1536, '1.5KB'],
    [2 * 1024 ** 2, '2.0MB'],
    [3 * 1024 ** 3, '3.0GB'],
    [4 * 1024 ** 4, '4.0TB'],
  ])('formats %i bytes as %s', (bytes, expected) => {
    expect(formatFileSize(bytes)).toBe(expected);
  });

  it.each([
    ['17', 17],
    ['1.5B', 2],
    ['1.5KB', 1536],
    ['2MB', 2 * 1024 ** 2],
    ['3GB', 3 * 1024 ** 3],
    ['4TB', 4 * 1024 ** 4],
    ['1K', 1024],
    ['2M', 2 * 1024 ** 2],
    ['3G', 3 * 1024 ** 3],
    ['4T', 4 * 1024 ** 4],
  ])('parses %s as %i bytes', (value, expected) => {
    expect(parseFileSize(value)).toBe(expected);
  });

  it('rejects malformed sizes', () => {
    expect(() => parseFileSize('many')).toThrow('Invalid size format: many');
  });
});
