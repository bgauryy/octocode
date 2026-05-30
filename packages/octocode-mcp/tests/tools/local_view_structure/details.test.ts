import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import fs from 'fs';
import path from 'path';
import os from 'os';
import {
  walkDirectory,
  type WalkStats,
} from '../../../src/tools/local_view_structure/structureWalker.js';
import {
  formatEntryString,
  type DirectoryEntry,
} from '../../../src/tools/local_view_structure/structureFilters.js';
import { parseLsLongFormat } from '../../../src/tools/local_view_structure/structureParser.js';

describe('localViewStructure details param', () => {
  let tmpDir: string;

  beforeEach(async () => {
    tmpDir = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'details-test-'));
    await fs.promises.writeFile(path.join(tmpDir, 'file.txt'), 'hello');
    await fs.promises.mkdir(path.join(tmpDir, 'subdir'));
  });

  afterEach(async () => {
    await fs.promises.rm(tmpDir, { recursive: true, force: true });
  });

  describe('walkDirectory with showDetails=true', () => {
    it('should populate modified field when showDetails is true', async () => {
      const entries: DirectoryEntry[] = [];
      const stats: WalkStats = { skipped: 0, permissionDenied: 0 };

      await walkDirectory({
        basePath: tmpDir,
        currentPath: tmpDir,
        depth: 0,
        maxDepth: 1,
        entries,
        maxEntries: 100,
        showHidden: false,
        showModified: false, // showModified=false
        stats,
        showDetails: true, // showDetails=true
      });

      const fileEntry = entries.find(e => e.name === 'file.txt');
      expect(fileEntry).toBeDefined();
      expect(fileEntry!.modified).toBeDefined();
      expect(fileEntry!.modified).toMatch(/^\d{4}-\d{2}-\d{2}T/); // ISO date
    });

    it('should populate permissions field when showDetails is true', async () => {
      const entries: DirectoryEntry[] = [];
      const stats: WalkStats = { skipped: 0, permissionDenied: 0 };

      await walkDirectory({
        basePath: tmpDir,
        currentPath: tmpDir,
        depth: 0,
        maxDepth: 1,
        entries,
        maxEntries: 100,
        showHidden: false,
        showModified: false, // showModified=false
        stats,
        showDetails: true, // showDetails=true
      });

      const fileEntry = entries.find(e => e.name === 'file.txt');
      expect(fileEntry).toBeDefined();
      expect(fileEntry!.permissions).toBeDefined();
      expect(fileEntry!.permissions).toMatch(
        /^[r-][w-][x-][r-][w-][x-][r-][w-][x-]$/
      );
    });

    it('should populate both modified and permissions for directories', async () => {
      const entries: DirectoryEntry[] = [];
      const stats: WalkStats = { skipped: 0, permissionDenied: 0 };

      await walkDirectory({
        basePath: tmpDir,
        currentPath: tmpDir,
        depth: 0,
        maxDepth: 1,
        entries,
        maxEntries: 100,
        showHidden: false,
        showModified: false,
        stats,
        showDetails: true,
      });

      const dirEntry = entries.find(e => e.name === 'subdir');
      expect(dirEntry).toBeDefined();
      expect(dirEntry!.type).toBe('directory');
      expect(dirEntry!.modified).toBeDefined();
      expect(dirEntry!.permissions).toBeDefined();
    });

    it('should NOT populate permissions when showDetails is false', async () => {
      const entries: DirectoryEntry[] = [];
      const stats: WalkStats = { skipped: 0, permissionDenied: 0 };

      await walkDirectory({
        basePath: tmpDir,
        currentPath: tmpDir,
        depth: 0,
        maxDepth: 1,
        entries,
        maxEntries: 100,
        showHidden: false,
        showModified: false,
        stats,
        showDetails: false, // showDetails=false
      });

      const fileEntry = entries.find(e => e.name === 'file.txt');
      expect(fileEntry).toBeDefined();
      expect(fileEntry!.permissions).toBeUndefined();
      expect(fileEntry!.modified).toBeUndefined();
    });
  });

  describe('formatEntryString with permissions', () => {
    it('should include permissions in output when present', () => {
      const entry: DirectoryEntry = {
        name: 'test.ts',
        type: 'file',
        size: '1.2K',
        permissions: 'rw-r--r--',
        modified: '2025-02-16T10:00:00.000Z',
        extension: 'ts',
      };

      const result = formatEntryString(entry, 0);
      expect(result).toContain('rw-r--r--');
      expect(result).toContain('2025-02-16');
      expect(result).toContain('test.ts');
    });

    it('should include permissions for directories', () => {
      const entry: DirectoryEntry = {
        name: 'src',
        type: 'directory',
        permissions: 'rwxr-xr-x',
        modified: '2025-02-16T10:00:00.000Z',
      };

      const result = formatEntryString(entry, 0);
      expect(result).toContain('rwxr-xr-x');
      expect(result).toContain('[DIR]');
    });

    it('should NOT include permissions when not present', () => {
      const entry: DirectoryEntry = {
        name: 'test.ts',
        type: 'file',
        size: '1.2K',
        extension: 'ts',
      };

      const result = formatEntryString(entry, 0);
      expect(result).not.toMatch(/[r-][w-][x-]{7}/);
      expect(result).toContain('test.ts');
    });
  });

  describe('parseLsLongFormat with details', () => {
    it('should always include modified when parsing long format', () => {
      // ls -l output format (macOS):
      const lsOutput = [
        'total 8',
        '-rw-r--r--  1 user  staff  1234 Feb 16 10:00 test.ts',
        'drwxr-xr-x  3 user  staff    96 Feb 16 09:00 src',
      ].join('\n');

      // Even with showModified=false, parseLsLongFormat is called from details=true path
      // so it should include modified
      const entries = parseLsLongFormat(lsOutput, true);

      expect(entries.length).toBe(2);
      const fileEntry = entries.find(e => e.name === 'test.ts');
      expect(fileEntry).toBeDefined();
      expect(fileEntry!.permissions).toBe('-rw-r--r--');
      expect(fileEntry!.modified).toBeDefined();
    });

    it('should include permissions in long format output', () => {
      const lsOutput = [
        'total 8',
        '-rw-r--r--  1 user  staff  1234 Feb 16 10:00 test.ts',
      ].join('\n');

      const entries = parseLsLongFormat(lsOutput, true);
      expect(entries[0]!.permissions).toBe('-rw-r--r--');
    });
  });
});
