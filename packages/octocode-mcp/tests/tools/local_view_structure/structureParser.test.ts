import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  parseLsSimple,
  parseLsLongFormat,
} from '../../../src/tools/local_view_structure/structureParser.js';
import fs from 'fs';

describe('structureParser - parseLsSimple', () => {
  let lstatSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    lstatSpy = vi.spyOn(fs.promises, 'lstat');
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('should return fallback entry with type file when lstat throws (line 33)', async () => {
    const basePath = '/test/path';
    const output = 'good.txt\nbad.txt';

    lstatSpy
      .mockResolvedValueOnce({
        isDirectory: () => false,
        isSymbolicLink: () => false,
        isFile: () => true,
        size: 100,
        mtime: new Date(),
      } as fs.Stats)
      .mockRejectedValueOnce(new Error('Permission denied'));

    const entries = await parseLsSimple(output, basePath, false);

    expect(entries).toHaveLength(2);
    expect(entries[0]!.name).toBe('good.txt');
    expect(entries[0]!.type).toBe('file');
    expect(entries[0]!.size).toBeDefined();

    expect(entries[1]!.name).toBe('bad.txt');
    expect(entries[1]!.type).toBe('file');
    expect(entries[1]!.extension).toBe('txt');
    expect(entries[1]!.size).toBeUndefined();
  });
});

describe('structureParser - parseLsLongFormat', () => {
  describe('line 22: skip total line', () => {
    it('should skip lines starting with "total "', () => {
      const output = `total 42
-rw-r--r-- 1 user group 1024 Jan 1 12:00 file.txt`;

      const entries = parseLsLongFormat(output, false);

      expect(entries).toHaveLength(1);
      expect(entries[0]!.name).toBe('file.txt');
    });
  });

  describe('line 58: parseFileSize for non-numeric size', () => {
    it('should use parseFileSize when size has unit suffix (4K, 1.5M)', () => {
      const output = `-rw-r--r-- 1 user group 4K Jan 1 12:00 small.bin
-rw-r--r-- 1 user group 1.5M Jan 1 12:00 large.bin`;

      const entries = parseLsLongFormat(output, false);

      expect(entries).toHaveLength(2);
      expect(entries[0]!.size).toBeDefined();
      expect(entries[1]!.size).toBeDefined();
      expect(entries[0]!.size).toMatch(/KB|B/);
      expect(entries[1]!.size).toMatch(/MB|KB/);
    });
  });

  describe('line 82: symlink detection', () => {
    it('should detect symlink when permissions start with l', () => {
      const output = `lrwxrwxrwx 1 user group 10 Jan 1 12:00 link -> target
-rw-r--r-- 1 user group 1024 Jan 1 12:00 file.txt`;

      const entries = parseLsLongFormat(output, false);

      expect(entries).toHaveLength(2);
      expect(entries[0]!.type).toBe('symlink');
      expect(entries[0]!.name).toBe('link -> target');
      expect(entries[1]!.type).toBe('file');
    });
  });
});
