import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import * as fs from 'fs';
import * as path from 'path';
import { validateLSPServerPath } from '../../src/lsp/validation.js';

vi.mock('fs', async () => {
  const actual = await vi.importActual<typeof fs>('fs');
  return {
    ...actual,
    realpathSync: vi.fn(),
    statSync: vi.fn(),
  };
});

describe('validateLSPServerPath', () => {
  const mockRealpathSync = vi.mocked(fs.realpathSync);
  const mockStatSync = vi.mocked(fs.statSync);

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('valid paths', () => {
    it('should return valid result for existing file', () => {
      const binPath = '/usr/local/bin/server';
      const baseDir = '/usr/local';

      mockRealpathSync.mockReturnValue(binPath);
      mockStatSync.mockReturnValue({ isFile: () => true } as fs.Stats);

      const result = validateLSPServerPath(binPath, baseDir);

      expect(result.isValid).toBe(true);
      expect(result.resolvedPath).toBe(binPath);
      expect(result.error).toBeUndefined();
    });

    it('should resolve relative paths correctly', () => {
      const binPath = './bin/server';
      const baseDir = '/usr/local/lib';
      const resolvedPath = path.resolve(baseDir, binPath);

      mockRealpathSync.mockReturnValue(resolvedPath);
      mockStatSync.mockReturnValue({ isFile: () => true } as fs.Stats);

      const result = validateLSPServerPath(binPath, baseDir);

      expect(result.isValid).toBe(true);
      expect(result.resolvedPath).toBe(resolvedPath);
    });
  });

  describe('path traversal detection', () => {
    it('should reject paths that escape base directory via traversal', () => {
      const binPath = '../../../etc/passwd';
      const baseDir = '/usr/local/lib';

      const result = validateLSPServerPath(binPath, baseDir);

      expect(result.isValid).toBe(false);
      expect(result.error).toContain('escapes base directory');
    });

    it('should allow paths that stay within base directory', () => {
      const binPath = './subdir/../bin/server';
      const baseDir = '/usr/local/lib';
      const normalizedPath = path.resolve(baseDir, binPath);

      mockRealpathSync.mockReturnValue(normalizedPath);
      mockStatSync.mockReturnValue({ isFile: () => true } as fs.Stats);

      const result = validateLSPServerPath(binPath, baseDir);

      expect(result.isValid).toBe(true);
    });
  });

  describe('file existence errors', () => {
    it('should return error for non-existent file (ENOENT)', () => {
      const binPath = '/usr/local/bin/nonexistent';
      const baseDir = '/usr/local';

      const error = new Error('ENOENT: no such file or directory') as Error & {
        code: string;
      };
      error.code = 'ENOENT';
      mockRealpathSync.mockImplementation(() => {
        throw error;
      });

      const result = validateLSPServerPath(binPath, baseDir);

      expect(result.isValid).toBe(false);
      expect(result.error).toContain('not found');
      expect(result.error).not.toContain(binPath);
    });

    it('should return error for symlink loop (ELOOP)', () => {
      const binPath = '/usr/local/bin/looplink';
      const baseDir = '/usr/local';

      const error = new Error('ELOOP: too many symbolic links') as Error & {
        code: string;
      };
      error.code = 'ELOOP';
      mockRealpathSync.mockImplementation(() => {
        throw error;
      });

      const result = validateLSPServerPath(binPath, baseDir);

      expect(result.isValid).toBe(false);
      expect(result.error).toContain('Symlink loop');
    });

    it('should return generic error for other realpath failures', () => {
      const binPath = '/usr/local/bin/server';
      const baseDir = '/usr/local';

      const error = new Error('EACCES: permission denied') as Error & {
        code: string;
      };
      error.code = 'EACCES';
      mockRealpathSync.mockImplementation(() => {
        throw error;
      });

      const result = validateLSPServerPath(binPath, baseDir);

      expect(result.isValid).toBe(false);
      expect(result.error).toContain('Cannot resolve');
      expect(result.error).not.toContain(binPath);
    });
  });

  describe('file type validation', () => {
    it('should reject directories', () => {
      const binPath = '/usr/local/bin';
      const baseDir = '/usr/local';

      mockRealpathSync.mockReturnValue(binPath);
      mockStatSync.mockReturnValue({ isFile: () => false } as fs.Stats);

      const result = validateLSPServerPath(binPath, baseDir);

      expect(result.isValid).toBe(false);
      expect(result.error).toContain('not a file');
    });

    it('should return error when stat fails', () => {
      const binPath = '/usr/local/bin/server';
      const baseDir = '/usr/local';

      mockRealpathSync.mockReturnValue(binPath);
      mockStatSync.mockImplementation(() => {
        throw new Error('stat failed');
      });

      const result = validateLSPServerPath(binPath, baseDir);

      expect(result.isValid).toBe(false);
      expect(result.error).toContain('Cannot stat');
      expect(result.error).not.toContain(binPath);
    });
  });

  describe('absolute vs relative paths', () => {
    it('should handle absolute paths without base directory check', () => {
      const binPath = '/opt/bin/server';
      const baseDir = '/usr/local';

      mockRealpathSync.mockReturnValue(binPath);
      mockStatSync.mockReturnValue({ isFile: () => true } as fs.Stats);

      const result = validateLSPServerPath(binPath, baseDir);

      expect(result.isValid).toBe(true);
    });
  });
});
