import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  applyEntryFilters,
  formatEntryString,
  type DirectoryEntry,
} from '../../../../octocode-tools-core/src/tools/local_view_structure/structureFilters.js';
import { checkRegexSafety } from '../../../../octocode-tools-core/src/utils/core/safeRegex.js';
import type { z } from 'zod';
import type { ViewStructureQuerySchema } from '@octocodeai/octocode-core/schemas';

type ViewStructureQuery = z.infer<typeof ViewStructureQuerySchema>;

vi.mock('../../../../octocode-tools-core/src/utils/core/safeRegex.js', () => ({
  checkRegexSafety: vi.fn(),
}));

describe('structureFilters - applyEntryFilters', () => {
  const baseQuery: ViewStructureQuery = {
    path: '/test',
  };

  const mockCheckRegexSafety = vi.mocked(checkRegexSafety);

  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('lines 51-57: glob pattern with unsafe regex or RegExp throw', () => {
    it('should fall back to literal includes when checkRegexSafety returns safe: false', () => {
      mockCheckRegexSafety.mockReturnValue({
        safe: false,
        reason: 'Nested quantifiers detected (potential ReDoS)',
      });

      const entries: DirectoryEntry[] = [
        { name: 'foo*bar.txt', type: 'file', extension: 'txt' },
        { name: 'other.txt', type: 'file', extension: 'txt' },
      ];

      const filtered = applyEntryFilters(entries, {
        ...baseQuery,
        pattern: 'foo*',
      });

      expect(filtered).toHaveLength(1);
      expect(filtered[0]!.name).toBe('foo*bar.txt');
    });

    it('should fall back to literal includes when RegExp constructor throws', () => {
      mockCheckRegexSafety.mockReturnValue({ safe: true });

      const RegExpSpy = vi.spyOn(global, 'RegExp').mockImplementation(() => {
        throw new Error('Invalid regular expression');
      });

      const entries: DirectoryEntry[] = [
        { name: 'needle*.txt', type: 'file', extension: 'txt' },
        { name: 'other.txt', type: 'file', extension: 'txt' },
      ];

      const filtered = applyEntryFilters(entries, {
        ...baseQuery,
        pattern: 'needle*',
      });

      RegExpSpy.mockRestore();

      expect(filtered).toHaveLength(1);
      expect(filtered[0]!.name).toBe('needle*.txt');
    });
  });

  describe('line 80: e.name.includes("/") in recursive mode', () => {
    it('should use split("/").pop() when entry name has path separator', () => {
      mockCheckRegexSafety.mockReturnValue({ safe: true });

      const entries: DirectoryEntry[] = [
        { name: 'subdir/nested/file.ts', type: 'file', extension: 'ts' },
        { name: 'subdir/other.txt', type: 'file', extension: 'txt' },
        { name: 'root.ts', type: 'file', extension: 'ts' },
      ];

      const filtered = applyEntryFilters(entries, {
        ...baseQuery,
        pattern: 'file',
      });

      expect(filtered).toHaveLength(1);
      expect(filtered[0]!.name).toBe('subdir/nested/file.ts');
    });
  });

  describe('line 126: query.extensions filter', () => {
    it('should filter by extension when query.extensions is set', () => {
      const entries: DirectoryEntry[] = [
        { name: 'foo.ts', type: 'file', extension: 'ts' },
        { name: 'bar.js', type: 'file', extension: 'js' },
        { name: 'dir', type: 'directory' },
      ];

      const filtered = applyEntryFilters(entries, {
        ...baseQuery,
        extensions: ['ts'],
      });

      expect(filtered).toHaveLength(2);
      expect(filtered.map(e => e.name)).toContain('foo.ts');
      expect(filtered.map(e => e.name)).toContain('dir');
      expect(filtered.map(e => e.name)).not.toContain('bar.js');
    });
  });

  describe('line 135: query.directoriesOnly filter', () => {
    it('should filter to directories only when directoriesOnly is true', () => {
      const entries: DirectoryEntry[] = [
        { name: 'file.txt', type: 'file', extension: 'txt' },
        { name: 'dir1', type: 'directory' },
        { name: 'dir2', type: 'directory' },
      ];

      const filtered = applyEntryFilters(entries, {
        ...baseQuery,
        directoriesOnly: true,
      });

      expect(filtered).toHaveLength(2);
      expect(filtered.every(e => e.type === 'directory')).toBe(true);
      expect(filtered.map(e => e.name)).toContain('dir1');
      expect(filtered.map(e => e.name)).toContain('dir2');
    });
  });
});

describe('structureFilters - formatEntryString', () => {
  describe('line 162: entry.type === "file" && entry.size vs else', () => {
    it('should use file+size branch when entry is file with size', () => {
      const entry: DirectoryEntry = {
        name: 'file.txt',
        type: 'file',
        size: '1.0KB',
        extension: 'txt',
      };
      const result = formatEntryString(entry);
      expect(result).toContain('(1.0KB)');
      expect(result).toContain('.txt');
    });

    it('should use else branch when entry is file without size', () => {
      const entry: DirectoryEntry = {
        name: 'file.txt',
        type: 'file',
        extension: 'txt',
      };
      const result = formatEntryString(entry);
      expect(result).not.toContain('(');
      expect(result).toContain('file.txt');
    });

    it('should use else branch when entry is directory', () => {
      const entry: DirectoryEntry = {
        name: 'dir',
        type: 'directory',
      };
      const result = formatEntryString(entry);
      expect(result).toContain('[DIR]');
      expect(result).toContain('dir/');
    });

    it('should use else branch when entry is symlink', () => {
      const entry: DirectoryEntry = {
        name: 'link',
        type: 'symlink',
      };
      const result = formatEntryString(entry);
      expect(result).toContain('[LINK]');
    });
  });
});
