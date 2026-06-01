import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import {
  dirExists,
  fileExists,
  readFileContent,
  writeFileContent,
  backupFile,
  readJsonFile,
  writeJsonFile,
  copyDirectory,
  listSubdirectories,
  removeDirectory,
} from '../../src/utils/fs.js';

describe('File System Utilities', () => {
  let tempDir: string;

  beforeEach(() => {
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'fs-test-'));
  });

  afterEach(() => {
    try {
      fs.rmSync(tempDir, { recursive: true, force: true });
    } catch {
      // ignore cleanup errors
    }
  });

  describe('dirExists', () => {
    it('should return true for existing directory', () => {
      expect(dirExists(tempDir)).toBe(true);
    });

    it('should return false for non-existing path', () => {
      expect(dirExists(path.join(tempDir, 'nonexistent'))).toBe(false);
    });

    it('should return false for file (not directory)', () => {
      const filePath = path.join(tempDir, 'file.txt');
      fs.writeFileSync(filePath, 'content');
      expect(dirExists(filePath)).toBe(false);
    });

    it('should return false on error (invalid path)', () => {
      expect(dirExists('\0')).toBe(false);
    });
  });

  describe('fileExists', () => {
    it('should return true for existing file', () => {
      const filePath = path.join(tempDir, 'file.txt');
      fs.writeFileSync(filePath, 'content');
      expect(fileExists(filePath)).toBe(true);
    });

    it('should return false for non-existing file', () => {
      expect(fileExists(path.join(tempDir, 'nonexistent.txt'))).toBe(false);
    });

    it('should return false for directory (not file)', () => {
      expect(fileExists(tempDir)).toBe(false);
    });

    it('should return false on error (invalid path)', () => {
      expect(fileExists('\0')).toBe(false);
    });
  });

  describe('readFileContent', () => {
    it('should return file content for existing file', () => {
      const filePath = path.join(tempDir, 'file.txt');
      fs.writeFileSync(filePath, 'file content');
      expect(readFileContent(filePath)).toBe('file content');
    });

    it('should return null for non-existing file', () => {
      expect(readFileContent(path.join(tempDir, 'nonexistent.txt'))).toBeNull();
    });

    it('should return null for directory', () => {
      expect(readFileContent(tempDir)).toBeNull();
    });
  });

  describe('writeFileContent', () => {
    it('should write content to file', () => {
      const filePath = path.join(tempDir, 'file.txt');
      const result = writeFileContent(filePath, 'new content');

      expect(result).toBe(true);
      expect(fs.readFileSync(filePath, 'utf8')).toBe('new content');
    });

    it('should create directory if it does not exist', () => {
      const filePath = path.join(tempDir, 'new', 'nested', 'file.txt');
      const result = writeFileContent(filePath, 'content');

      expect(result).toBe(true);
      expect(fs.existsSync(path.join(tempDir, 'new', 'nested'))).toBe(true);
      expect(fs.readFileSync(filePath, 'utf8')).toBe('content');
    });

    it('should overwrite existing file', () => {
      const filePath = path.join(tempDir, 'file.txt');
      fs.writeFileSync(filePath, 'old content');
      const result = writeFileContent(filePath, 'new content');

      expect(result).toBe(true);
      expect(fs.readFileSync(filePath, 'utf8')).toBe('new content');
    });

    it('should return false on write error (invalid path)', () => {
      const result = writeFileContent('\0', 'content');
      expect(result).toBe(false);
    });
  });

  describe('backupFile', () => {
    it('should create backup of existing file', () => {
      const filePath = path.join(tempDir, 'file.txt');
      fs.writeFileSync(filePath, 'original content');

      const backupPath = backupFile(filePath);

      expect(backupPath).not.toBeNull();
      expect(backupPath).toMatch(
        /file\.txt\.backup-\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}-\d{3}Z$/
      );
      expect(fs.existsSync(backupPath!)).toBe(true);
      expect(fs.readFileSync(backupPath!, 'utf8')).toBe('original content');
    });

    it('should return null for non-existing file', () => {
      const result = backupFile(path.join(tempDir, 'nonexistent.txt'));
      expect(result).toBeNull();
    });

    it('should return null for directory', () => {
      const result = backupFile(tempDir);
      expect(result).toBeNull();
    });
  });

  describe('readJsonFile', () => {
    it('should parse and return JSON content', () => {
      const filePath = path.join(tempDir, 'file.json');
      fs.writeFileSync(filePath, '{"key": "value"}');

      const result = readJsonFile<{ key: string }>(filePath);
      expect(result).toEqual({ key: 'value' });
    });

    it('should handle complex JSON structures', () => {
      const filePath = path.join(tempDir, 'complex.json');
      const data = { arr: [1, 2, 3], nested: { a: true, b: null } };
      fs.writeFileSync(filePath, JSON.stringify(data));

      const result = readJsonFile<typeof data>(filePath);
      expect(result).toEqual(data);
    });

    it('should return null for invalid JSON', () => {
      const filePath = path.join(tempDir, 'invalid.json');
      fs.writeFileSync(filePath, 'not valid json');

      const result = readJsonFile(filePath);
      expect(result).toBeNull();
    });

    it('should return null for non-existing file', () => {
      const result = readJsonFile(path.join(tempDir, 'nonexistent.json'));
      expect(result).toBeNull();
    });

    it('should return null for empty file', () => {
      const filePath = path.join(tempDir, 'empty.json');
      fs.writeFileSync(filePath, '');

      const result = readJsonFile(filePath);
      expect(result).toBeNull();
    });
  });

  describe('writeJsonFile', () => {
    it('should write formatted JSON to file', () => {
      const filePath = path.join(tempDir, 'file.json');
      const result = writeJsonFile(filePath, { key: 'value' });

      expect(result).toBe(true);
      expect(fs.readFileSync(filePath, 'utf8')).toBe(
        '{\n  "key": "value"\n}\n'
      );
    });

    it('should handle arrays', () => {
      const filePath = path.join(tempDir, 'array.json');
      const result = writeJsonFile(filePath, [1, 2, 3]);

      expect(result).toBe(true);
      expect(fs.readFileSync(filePath, 'utf8')).toBe('[\n  1,\n  2,\n  3\n]\n');
    });

    it('should create directory if it does not exist', () => {
      const filePath = path.join(tempDir, 'new', 'file.json');
      const result = writeJsonFile(filePath, { key: 'value' });

      expect(result).toBe(true);
      expect(fs.existsSync(filePath)).toBe(true);
    });

    it('should return false on write error (invalid path)', () => {
      const result = writeJsonFile('\0', { key: 'value' });
      expect(result).toBe(false);
    });

    it('should return false for circular reference', () => {
      const circular: Record<string, unknown> = {};
      circular.self = circular;

      const filePath = path.join(tempDir, 'circular.json');
      const result = writeJsonFile(filePath, circular);
      expect(result).toBe(false);
    });
  });

  describe('copyDirectory', () => {
    it('should copy directory recursively', () => {
      const srcDir = path.join(tempDir, 'src');
      fs.mkdirSync(srcDir);
      fs.writeFileSync(path.join(srcDir, 'file1.txt'), 'content1');
      fs.mkdirSync(path.join(srcDir, 'subdir'));
      fs.writeFileSync(path.join(srcDir, 'subdir', 'file2.txt'), 'content2');

      const destDir = path.join(tempDir, 'dest');
      const result = copyDirectory(srcDir, destDir);

      expect(result).toBe(true);
      expect(fs.existsSync(destDir)).toBe(true);
      expect(fs.readFileSync(path.join(destDir, 'file1.txt'), 'utf8')).toBe(
        'content1'
      );
      expect(
        fs.readFileSync(path.join(destDir, 'subdir', 'file2.txt'), 'utf8')
      ).toBe('content2');
    });

    it('should return false if source does not exist', () => {
      const result = copyDirectory(
        path.join(tempDir, 'nonexistent'),
        path.join(tempDir, 'dest')
      );
      expect(result).toBe(false);
    });

    it('should create destination directory if it does not exist', () => {
      const srcDir = path.join(tempDir, 'src');
      fs.mkdirSync(srcDir);
      fs.writeFileSync(path.join(srcDir, 'file.txt'), 'content');

      const destDir = path.join(tempDir, 'new', 'dest');
      const result = copyDirectory(srcDir, destDir);

      expect(result).toBe(true);
      expect(fs.existsSync(destDir)).toBe(true);
    });

    it('should handle empty directories', () => {
      const srcDir = path.join(tempDir, 'empty-src');
      fs.mkdirSync(srcDir);

      const destDir = path.join(tempDir, 'empty-dest');
      const result = copyDirectory(srcDir, destDir);

      expect(result).toBe(true);
      expect(fs.existsSync(destDir)).toBe(true);
    });

    it('should return false when source is a file', () => {
      const srcFile = path.join(tempDir, 'file.txt');
      fs.writeFileSync(srcFile, 'content');

      const result = copyDirectory(srcFile, path.join(tempDir, 'dest'));
      expect(result).toBe(false);
    });

    it('should work when destination directory already exists', () => {
      const srcDir = path.join(tempDir, 'existing-src');
      fs.mkdirSync(srcDir);
      fs.writeFileSync(path.join(srcDir, 'file.txt'), 'content');

      const destDir = path.join(tempDir, 'existing-dest');
      fs.mkdirSync(destDir);

      const result = copyDirectory(srcDir, destDir);

      expect(result).toBe(true);
      expect(fs.readFileSync(path.join(destDir, 'file.txt'), 'utf8')).toBe(
        'content'
      );
    });
  });

  describe('listSubdirectories', () => {
    it('should return list of subdirectories', () => {
      fs.mkdirSync(path.join(tempDir, 'dir1'));
      fs.mkdirSync(path.join(tempDir, 'dir2'));
      fs.writeFileSync(path.join(tempDir, 'file.txt'), 'content');

      const result = listSubdirectories(tempDir);

      expect(result).toContain('dir1');
      expect(result).toContain('dir2');
      expect(result).not.toContain('file.txt');
      expect(result).toHaveLength(2);
    });

    it('should return empty array for non-existing directory', () => {
      const result = listSubdirectories(path.join(tempDir, 'nonexistent'));
      expect(result).toEqual([]);
    });

    it('should return empty array for empty directory', () => {
      const emptyDir = path.join(tempDir, 'empty');
      fs.mkdirSync(emptyDir);

      const result = listSubdirectories(emptyDir);
      expect(result).toEqual([]);
    });

    it('should return empty array for file (not directory)', () => {
      const filePath = path.join(tempDir, 'file.txt');
      fs.writeFileSync(filePath, 'content');

      const result = listSubdirectories(filePath);
      expect(result).toEqual([]);
    });

    it('should return empty array on error (invalid path)', () => {
      const result = listSubdirectories('\0');
      expect(result).toEqual([]);
    });

    it('should include symlinked directories', () => {
      const realDir = path.join(tempDir, 'real-dir');
      const linkDir = path.join(tempDir, 'link-dir');
      fs.mkdirSync(realDir);
      fs.symlinkSync(realDir, linkDir);

      const result = listSubdirectories(tempDir);

      expect(result).toContain('real-dir');
      expect(result).toContain('link-dir');
    });

    it('should skip broken symlinks', () => {
      const brokenLink = path.join(tempDir, 'broken-link');
      fs.symlinkSync(path.join(tempDir, 'nonexistent'), brokenLink);

      const result = listSubdirectories(tempDir);

      expect(result).not.toContain('broken-link');
    });
  });

  describe('error handling with mocks', () => {
    it('dirExists should return false when statSync throws', () => {
      const existsSyncSpy = vi.spyOn(fs, 'existsSync').mockReturnValue(true);
      const statSyncSpy = vi.spyOn(fs, 'statSync').mockImplementation(() => {
        throw new Error('Permission denied');
      });

      expect(dirExists('/some/path')).toBe(false);

      existsSyncSpy.mockRestore();
      statSyncSpy.mockRestore();
    });

    it('fileExists should return false when statSync throws', () => {
      const existsSyncSpy = vi.spyOn(fs, 'existsSync').mockReturnValue(true);
      const statSyncSpy = vi.spyOn(fs, 'statSync').mockImplementation(() => {
        throw new Error('Permission denied');
      });

      expect(fileExists('/some/path')).toBe(false);

      existsSyncSpy.mockRestore();
      statSyncSpy.mockRestore();
    });

    it('backupFile should return null when copyFileSync throws', () => {
      const filePath = path.join(tempDir, 'backup-test.txt');
      fs.writeFileSync(filePath, 'content');

      const copyFileSyncSpy = vi
        .spyOn(fs, 'copyFileSync')
        .mockImplementation(() => {
          throw new Error('Copy failed');
        });

      expect(backupFile(filePath)).toBeNull();

      copyFileSyncSpy.mockRestore();
    });

    it('copyDirectory should return false when readdirSync throws', () => {
      const srcDir = path.join(tempDir, 'copy-error-src');
      fs.mkdirSync(srcDir);

      const readdirSyncSpy = vi
        .spyOn(fs, 'readdirSync')
        .mockImplementation(() => {
          throw new Error('Read failed');
        });

      expect(copyDirectory(srcDir, path.join(tempDir, 'copy-error-dest'))).toBe(
        false
      );

      readdirSyncSpy.mockRestore();
    });

    it('listSubdirectories should return empty array when readdirSync throws', () => {
      const readdirSyncSpy = vi
        .spyOn(fs, 'readdirSync')
        .mockImplementation(() => {
          throw new Error('Read failed');
        });

      expect(listSubdirectories(tempDir)).toEqual([]);

      readdirSyncSpy.mockRestore();
    });

    it('copyDirectory returns false when recursive subdirectory copy fails', () => {
      const srcDir = path.join(tempDir, 'nested-src');
      const subDir = path.join(srcDir, 'sub');
      fs.mkdirSync(subDir, { recursive: true });

      // First readdirSync returns the subdirectory; second throws (recursive copy fails)
      let callCount = 0;
      const readdirSyncSpy = vi
        .spyOn(fs, 'readdirSync')
        // Cast to `never`: readdirSync has many overloads (the buffer-encoding
        // one expects Dirent<NonSharedBuffer>[]); the test only needs a stub
        // that returns one Dirent then throws.
        .mockImplementation(((_p: unknown, _opts: unknown) => {
          callCount++;
          if (callCount === 1) {
            return [
              {
                name: 'sub',
                isDirectory: () => true,
                isFile: () => false,
              } as fs.Dirent,
            ];
          }
          throw new Error('Subdirectory read failed');
        }) as never);

      expect(copyDirectory(srcDir, path.join(tempDir, 'nested-dest'))).toBe(
        false
      );

      readdirSyncSpy.mockRestore();
    });

    it('removeDirectory returns false when dir does not exist', () => {
      expect(removeDirectory('/nonexistent/path/12345')).toBe(false);
    });

    it('removeDirectory removes dir and returns true when it exists', () => {
      const dir = path.join(tempDir, 'to-remove');
      fs.mkdirSync(dir);
      expect(removeDirectory(dir)).toBe(true);
      expect(fs.existsSync(dir)).toBe(false);
    });
  });
});
