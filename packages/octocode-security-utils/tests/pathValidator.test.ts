/**
 * Tests for pathValidator.ts
 * Covers PathValidator class, validation, error handling, and edge cases
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  PathValidator,
  pathValidator,
  reinitializePathValidator,
} from '../src/pathValidator.js';
import os from 'os';
import fs from 'fs';

interface ErrnoException extends Error {
  code?: string;
}

describe('PathValidator', () => {
  const testWorkspace = process.cwd();
  let validator: PathValidator;

  beforeEach(() => {
    validator = new PathValidator({ workspaceRoot: testWorkspace });
  });

  describe('constructor', () => {
    it('should accept PathValidatorOptions object', () => {
      const v = new PathValidator({
        workspaceRoot: testWorkspace,
        includeHomeDir: true,
      });
      const result = v.validate(`${testWorkspace}/package.json`);
      expect(result.isValid).toBe(true);
    });

    it('should use cwd when no workspace root provided', () => {
      const v = new PathValidator();
      const cwd = process.cwd();
      const result = v.validate(`${cwd}/package.json`);
      expect(result.isValid).toBe(true);
    });

    it('should include home directory by default', () => {
      const v = new PathValidator({ workspaceRoot: testWorkspace });
      const homeDir = os.homedir();
      const roots = v.getAllowedRoots();
      expect(roots).toContain(homeDir);
    });

    it('should exclude home directory when includeHomeDir is false', () => {
      const v = new PathValidator({
        workspaceRoot: testWorkspace,
        includeHomeDir: false,
      });
      const homeDir = os.homedir();
      const roots = v.getAllowedRoots();
      // Home dir should only be included if it happens to equal workspace
      const expectedInclusion = testWorkspace.startsWith(homeDir);
      if (!expectedInclusion) {
        expect(roots).not.toContain(homeDir);
      }
    });

    it('should add additional roots from options', () => {
      const additionalRoot = '/tmp';
      const v = new PathValidator({
        workspaceRoot: testWorkspace,
        additionalRoots: [additionalRoot],
      });
      const roots = v.getAllowedRoots();
      expect(roots).toContain(additionalRoot);
    });

    it('should expand tilde in workspace root', () => {
      const v = new PathValidator({
        workspaceRoot: '~',
      });
      const homeDir = os.homedir();
      const roots = v.getAllowedRoots();
      expect(roots).toContain(homeDir);
    });

    it('should read ALLOWED_PATHS environment variable', () => {
      const originalEnv = process.env.ALLOWED_PATHS;
      process.env.ALLOWED_PATHS = '/tmp,/var';

      try {
        const v = new PathValidator({ workspaceRoot: testWorkspace });
        const roots = v.getAllowedRoots();
        expect(roots).toContain('/tmp');
        expect(roots).toContain('/var');
      } finally {
        if (originalEnv === undefined) {
          delete process.env.ALLOWED_PATHS;
        } else {
          process.env.ALLOWED_PATHS = originalEnv;
        }
      }
    });

    it('should handle empty strings in ALLOWED_PATHS', () => {
      const originalEnv = process.env.ALLOWED_PATHS;
      process.env.ALLOWED_PATHS = '/tmp,  ,/var, ';

      try {
        const v = new PathValidator({ workspaceRoot: testWorkspace });
        const roots = v.getAllowedRoots();
        expect(roots).toContain('/tmp');
        expect(roots).toContain('/var');
      } finally {
        if (originalEnv === undefined) {
          delete process.env.ALLOWED_PATHS;
        } else {
          process.env.ALLOWED_PATHS = originalEnv;
        }
      }
    });
  });

  describe('validate', () => {
    it('should reject empty path', () => {
      const result = validator.validate('');
      expect(result.isValid).toBe(false);
      expect(result.error).toContain('empty');
    });

    it('should reject whitespace-only path', () => {
      const result = validator.validate('   ');
      expect(result.isValid).toBe(false);
      expect(result.error).toContain('empty');
    });

    it('should accept valid path within workspace', () => {
      const result = validator.validate(`${testWorkspace}/package.json`);
      expect(result.isValid).toBe(true);
      expect(result.sanitizedPath).toBeDefined();
    });

    it('should accept workspace root itself', () => {
      const result = validator.validate(testWorkspace);
      expect(result.isValid).toBe(true);
    });

    it('should reject path outside workspace', () => {
      const strictValidator = new PathValidator({
        workspaceRoot: testWorkspace,
        includeHomeDir: false,
      });
      const result = strictValidator.validate('/etc/passwd');
      expect(result.isValid).toBe(false);
      expect(result.error).toContain('outside allowed directories');
    });

    it('should redact absolute path in outside-workspace error message', () => {
      const strictValidator = new PathValidator({
        workspaceRoot: testWorkspace,
        includeHomeDir: false,
      });
      const secretPath = '/var/private/secrets/important.key';
      const result = strictValidator.validate(secretPath);
      expect(result.isValid).toBe(false);
      // Error should NOT leak the full absolute path
      expect(result.error).not.toContain('/var/private/secrets');
      // Should contain a redacted form (basename fallback)
      expect(result.error).toContain('important.key');
    });

    it('should redact path in ignored-path error message', () => {
      const result = validator.validate(`${testWorkspace}/.env`);
      expect(result.isValid).toBe(false);
      expect(result.error).toContain('ignored');
      // Error should contain project-relative path, not absolute
      expect(result.error).not.toContain(testWorkspace);
      expect(result.error).toContain('.env');
    });

    it('should redact path in traversal error message', () => {
      const strictValidator = new PathValidator({
        workspaceRoot: testWorkspace,
        includeHomeDir: false,
      });
      const traversalPath = `${testWorkspace}/../../../etc/shadow`;
      const result = strictValidator.validate(traversalPath);
      expect(result.isValid).toBe(false);
      // Error must NOT contain the full absolute workspace path (it should be redacted)
      expect(result.error).not.toContain(testWorkspace);
      // Error must NOT contain the full original traversal path
      expect(result.error).not.toContain(traversalPath);
      // Should still indicate the path is outside allowed dirs
      expect(result.error).toContain('outside allowed directories');
    });

    it('should handle tilde expansion in validate', () => {
      const result = validator.validate('~/');
      expect(result.isValid).toBe(true);
    });

    it('should reject ignored paths (.env)', () => {
      const result = validator.validate(`${testWorkspace}/.env`);
      expect(result.isValid).toBe(false);
      expect(result.error).toContain('ignored');
    });

    it('should handle non-existent paths (ENOENT)', () => {
      const result = validator.validate(
        `${testWorkspace}/non-existent-file-xyz-123.txt`
      );
      // Non-existent paths within workspace should be valid
      expect(result.isValid).toBe(true);
      expect(result.sanitizedPath).toBeDefined();
    });

    it('should handle path with normalized double slashes', () => {
      const result = validator.validate(`${testWorkspace}//src//index.ts`);
      expect(result.isValid).toBe(true);
    });
  });

  describe('addAllowedRoot', () => {
    it('should add a new allowed root', () => {
      const newRoot = '/tmp';
      validator.addAllowedRoot(newRoot);
      const roots = validator.getAllowedRoots();
      expect(roots).toContain(newRoot);
    });

    it('should not add duplicate roots', () => {
      validator.addAllowedRoot(testWorkspace);
      validator.addAllowedRoot(testWorkspace);
      const roots = validator.getAllowedRoots();
      const count = roots.filter(r => r === testWorkspace).length;
      expect(count).toBe(1);
    });

    it('should expand tilde in added root', () => {
      validator.addAllowedRoot('~/test-dir');
      const roots = validator.getAllowedRoots();
      const homeDir = os.homedir();
      expect(roots.some(r => r.startsWith(homeDir))).toBe(true);
    });

    it('should resolve relative paths', () => {
      validator.addAllowedRoot('.');
      const roots = validator.getAllowedRoots();
      expect(roots).toContain(process.cwd());
    });
  });

  describe('exists', () => {
    it('should return true for existing files', async () => {
      const result = await validator.exists(`${testWorkspace}/package.json`);
      expect(result).toBe(true);
    });

    it('should return false for non-existing files', async () => {
      const result = await validator.exists(
        `${testWorkspace}/non-existent-xyz.txt`
      );
      expect(result).toBe(false);
    });

    it('should return false for invalid paths', async () => {
      const result = await validator.exists('');
      expect(result).toBe(false);
    });

    it('should return false for paths outside workspace', async () => {
      const strictValidator = new PathValidator({
        workspaceRoot: testWorkspace,
        includeHomeDir: false,
      });
      const result = await strictValidator.exists('/etc/passwd');
      expect(result).toBe(false);
    });
  });

  describe('getType', () => {
    it('should return "file" for regular files', async () => {
      const result = await validator.getType(`${testWorkspace}/package.json`);
      expect(result).toBe('file');
    });

    it('should return "directory" for directories', async () => {
      const result = await validator.getType(`${testWorkspace}/src`);
      expect(result).toBe('directory');
    });

    it('should return null for non-existing paths', async () => {
      const result = await validator.getType(
        `${testWorkspace}/non-existent-xyz`
      );
      expect(result).toBe(null);
    });

    it('should return null for invalid paths', async () => {
      const result = await validator.getType('');
      expect(result).toBe(null);
    });

    it('should return null for paths outside workspace', async () => {
      const strictValidator = new PathValidator({
        workspaceRoot: testWorkspace,
        includeHomeDir: false,
      });
      const result = await strictValidator.getType('/etc/passwd');
      expect(result).toBe(null);
    });
  });

  describe('getAllowedRoots', () => {
    it('should return a copy of allowed roots', () => {
      const roots = validator.getAllowedRoots();
      expect(Array.isArray(roots)).toBe(true);
      expect(roots.length).toBeGreaterThan(0);
    });

    it('should be immutable', () => {
      const roots1 = validator.getAllowedRoots();
      const roots2 = validator.getAllowedRoots();
      // Should be different array instances
      expect(roots1).not.toBe(roots2);
      expect(roots1).toEqual(roots2);
    });
  });

  describe('reinitializePathValidator', () => {
    afterEach(() => {
      // Restore default state
      reinitializePathValidator();
    });

    it('should reinitialize the global validator', () => {
      const customRoot = '/tmp';
      reinitializePathValidator({
        workspaceRoot: customRoot,
        includeHomeDir: false,
      });

      const roots = pathValidator.getAllowedRoots();
      expect(roots).toContain(customRoot);
    });

    it('should return the pathValidator singleton', () => {
      const result = reinitializePathValidator();
      expect(result).toBe(pathValidator);
    });

    it('should update allowed roots of singleton', () => {
      const newValidator = new PathValidator({
        workspaceRoot: '/tmp',
        additionalRoots: ['/var'],
        includeHomeDir: false,
      });

      reinitializePathValidator({
        workspaceRoot: '/tmp',
        additionalRoots: ['/var'],
        includeHomeDir: false,
      });

      expect(pathValidator.getAllowedRoots()).toEqual(
        newValidator.getAllowedRoots()
      );
    });
  });

  describe('Error handling edge cases', () => {
    it('should handle path traversal attempts', () => {
      const strictValidator = new PathValidator({
        workspaceRoot: testWorkspace,
        includeHomeDir: false,
      });
      const result = strictValidator.validate(
        `${testWorkspace}/../../../etc/passwd`
      );
      expect(result.isValid).toBe(false);
    });

    it('should handle workspace root validation correctly', () => {
      // Test that workspace root itself is valid
      const testValidator = new PathValidator({
        workspaceRoot: testWorkspace,
        includeHomeDir: false,
      });

      const result = testValidator.validate(testWorkspace);
      expect(result.isValid).toBe(true);
    });
  });

  describe('Symlink error codes', () => {
    it('should mock EACCES error handling', async () => {
      // Create a mock validator and test EACCES handling
      const mockRealpathSync = vi
        .spyOn(fs, 'realpathSync')
        .mockImplementation(() => {
          const error = new Error('Permission denied') as ErrnoException;
          error.code = 'EACCES';
          throw error;
        });

      try {
        const v = new PathValidator({ workspaceRoot: testWorkspace });
        const result = v.validate(`${testWorkspace}/some-file`);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('Permission denied');
      } finally {
        mockRealpathSync.mockRestore();
      }
    });

    it('should mock ELOOP error handling', async () => {
      const mockRealpathSync = vi
        .spyOn(fs, 'realpathSync')
        .mockImplementation(() => {
          const error = new Error('Too many symbolic links') as ErrnoException;
          error.code = 'ELOOP';
          throw error;
        });

      try {
        const v = new PathValidator({ workspaceRoot: testWorkspace });
        const result = v.validate(`${testWorkspace}/some-file`);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('Symlink loop');
      } finally {
        mockRealpathSync.mockRestore();
      }
    });

    it('should mock ENAMETOOLONG error handling', async () => {
      const mockRealpathSync = vi
        .spyOn(fs, 'realpathSync')
        .mockImplementation(() => {
          const error = new Error('Name too long') as ErrnoException;
          error.code = 'ENAMETOOLONG';
          throw error;
        });

      try {
        const v = new PathValidator({ workspaceRoot: testWorkspace });
        const result = v.validate(`${testWorkspace}/some-file`);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('Path name too long');
      } finally {
        mockRealpathSync.mockRestore();
      }
    });

    it('should handle unknown error codes', async () => {
      const mockRealpathSync = vi
        .spyOn(fs, 'realpathSync')
        .mockImplementation(() => {
          const error = new Error('Unknown error') as ErrnoException;
          error.code = 'UNKNOWN';
          throw error;
        });

      try {
        const v = new PathValidator({ workspaceRoot: testWorkspace });
        const result = v.validate(`${testWorkspace}/some-file`);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('Unexpected error');
      } finally {
        mockRealpathSync.mockRestore();
      }
    });

    it('should handle non-Error exceptions', async () => {
      const mockRealpathSync = vi
        .spyOn(fs, 'realpathSync')
        .mockImplementation(() => {
          throw 'string error';
        });

      try {
        const v = new PathValidator({ workspaceRoot: testWorkspace });
        const result = v.validate(`${testWorkspace}/some-file`);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('Unexpected error');
      } finally {
        mockRealpathSync.mockRestore();
      }
    });
  });

  describe('Symlink target validation', () => {
    it('should reject symlink target outside workspace', async () => {
      const mockRealpathSync = vi
        .spyOn(fs, 'realpathSync')
        .mockReturnValue('/etc/passwd');

      try {
        const strictValidator = new PathValidator({
          workspaceRoot: testWorkspace,
          includeHomeDir: false,
        });
        const result = strictValidator.validate(`${testWorkspace}/link`);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('Symlink target');
      } finally {
        mockRealpathSync.mockRestore();
      }
    });

    it('should redact symlink target path in error message', () => {
      // Mock realpathSync to return a path outside workspace
      const outsidePath = '/etc/secrets/private.key';
      const mockRealpathSync = vi
        .spyOn(fs, 'realpathSync')
        .mockReturnValue(outsidePath);

      try {
        const strictValidator = new PathValidator({
          workspaceRoot: testWorkspace,
          includeHomeDir: false,
        });
        const result = strictValidator.validate(`${testWorkspace}/link`);
        expect(result.isValid).toBe(false);
        // Error should NOT contain the full absolute path
        expect(result.error).not.toContain('/etc/secrets/private.key');
        // Error should still indicate it's about a symlink target
        expect(result.error).toContain('Symlink target');
        expect(result.error).toContain('outside allowed directories');
      } finally {
        mockRealpathSync.mockRestore();
      }
    });

    it('should reject symlink target in ignored path', async () => {
      // Mock realpathSync to return a .git path
      const gitPath = `${testWorkspace}/.git/config`;
      const mockRealpathSync = vi
        .spyOn(fs, 'realpathSync')
        .mockReturnValue(gitPath);

      try {
        const v = new PathValidator({ workspaceRoot: testWorkspace });
        const result = v.validate(`${testWorkspace}/link-to-git`);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('ignored');
      } finally {
        mockRealpathSync.mockRestore();
      }
    });
  });

  describe('Error message path redaction', () => {
    it('should redact paths in EACCES error messages', () => {
      const mockRealpathSync = vi
        .spyOn(fs, 'realpathSync')
        .mockImplementation(() => {
          const error = new Error('Permission denied') as ErrnoException;
          error.code = 'EACCES';
          throw error;
        });

      try {
        const v = new PathValidator({ workspaceRoot: testWorkspace });
        const result = v.validate(`${testWorkspace}/protected-file`);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('Permission denied');
        // Error should NOT contain the full workspace path
        expect(result.error).not.toContain(testWorkspace);
      } finally {
        mockRealpathSync.mockRestore();
      }
    });

    it('should redact paths in ELOOP error messages', () => {
      const mockRealpathSync = vi
        .spyOn(fs, 'realpathSync')
        .mockImplementation(() => {
          const error = new Error('Too many symbolic links') as ErrnoException;
          error.code = 'ELOOP';
          throw error;
        });

      try {
        const v = new PathValidator({ workspaceRoot: testWorkspace });
        const result = v.validate(`${testWorkspace}/loop-link`);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('Symlink loop');
        // Error should NOT contain the full workspace path
        expect(result.error).not.toContain(testWorkspace);
      } finally {
        mockRealpathSync.mockRestore();
      }
    });

    it('should redact paths in ENAMETOOLONG error messages', () => {
      const mockRealpathSync = vi
        .spyOn(fs, 'realpathSync')
        .mockImplementation(() => {
          const error = new Error('Name too long') as ErrnoException;
          error.code = 'ENAMETOOLONG';
          throw error;
        });

      try {
        const v = new PathValidator({ workspaceRoot: testWorkspace });
        const result = v.validate(`${testWorkspace}/very-long-name`);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('Path name too long');
        // Error should NOT contain the full workspace path
        expect(result.error).not.toContain(testWorkspace);
      } finally {
        mockRealpathSync.mockRestore();
      }
    });

    it('should redact paths in unexpected error messages', () => {
      const mockRealpathSync = vi
        .spyOn(fs, 'realpathSync')
        .mockImplementation(() => {
          const error = new Error('Something went wrong') as ErrnoException;
          error.code = 'UNKNOWN';
          throw error;
        });

      try {
        const v = new PathValidator({ workspaceRoot: testWorkspace });
        const result = v.validate(`${testWorkspace}/problem-file`);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('Unexpected error');
        // Error should NOT contain the full workspace path
        expect(result.error).not.toContain(testWorkspace);
      } finally {
        mockRealpathSync.mockRestore();
      }
    });

    it('should redact symlink-to-ignored-path error messages', () => {
      const gitPath = `${testWorkspace}/.git/config`;
      const mockRealpathSync = vi
        .spyOn(fs, 'realpathSync')
        .mockReturnValue(gitPath);

      try {
        const v = new PathValidator({ workspaceRoot: testWorkspace });
        const result = v.validate(`${testWorkspace}/link-to-git`);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('ignored');
        // Error should NOT contain the full absolute path
        expect(result.error).not.toContain(testWorkspace);
      } finally {
        mockRealpathSync.mockRestore();
      }
    });
  });
});
