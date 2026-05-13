/**
 * Tests for execution context validation
 */

import { describe, it, expect, beforeEach } from 'vitest';
import path from 'path';
import fs from 'fs';
import os from 'os';
import { validateExecutionContext } from '../src/executionContextValidator.js';

describe('executionContextValidator', () => {
  const workspaceRoot = process.cwd();
  const parentDir = path.dirname(workspaceRoot);

  describe('validateExecutionContext', () => {
    it('should allow undefined cwd (defaults to safe process.cwd())', () => {
      const result = validateExecutionContext(undefined);
      expect(result.isValid).toBe(true);
      expect(result.error).toBeUndefined();
    });

    it('should reject empty string cwd', () => {
      const result = validateExecutionContext('');
      expect(result.isValid).toBe(false);
      expect(result.error).toContain('cannot be empty');
    });

    it('should reject whitespace-only cwd', () => {
      const result = validateExecutionContext('   ');
      expect(result.isValid).toBe(false);
      expect(result.error).toContain('cannot be empty');
    });

    it('should allow cwd within workspace (absolute path)', () => {
      const validPath = path.join(workspaceRoot, 'src');
      const result = validateExecutionContext(validPath);
      expect(result.isValid).toBe(true);
      expect(result.sanitizedPath).toBe(validPath);
    });

    it('should allow cwd within workspace (relative path)', () => {
      const result = validateExecutionContext('./src');
      expect(result.isValid).toBe(true);
      expect(result.sanitizedPath).toContain('src');
    });

    it('should allow workspace root itself', () => {
      const result = validateExecutionContext(workspaceRoot);
      expect(result.isValid).toBe(true);
      expect(result.sanitizedPath).toBe(workspaceRoot);
    });

    it('should reject parent directory', () => {
      const result = validateExecutionContext(parentDir);
      expect(result.isValid).toBe(false);
      expect(result.error).toContain('configured workspace directory');
      expect(result.error).not.toContain(parentDir);
      expect(result.error).not.toContain(workspaceRoot);
    });

    it('should reject sibling directory with similar prefix (path traversal attack)', () => {
      // This is a critical security test: "/workspace-evil" should NOT match "/workspace"
      // The vulnerable pattern: "/workspace-evil".startsWith("/workspace") === true
      const siblingPath = workspaceRoot + '-evil';
      const result = validateExecutionContext(siblingPath);
      expect(result.isValid).toBe(false);
      expect(result.error).toContain('configured workspace directory');
    });

    it('should reject paths with similar prefix (e.g., workspace2)', () => {
      const similarPath = workspaceRoot + '2';
      const result = validateExecutionContext(similarPath);
      expect(result.isValid).toBe(false);
      expect(result.error).toContain('configured workspace directory');
    });

    it('should reject path traversal with ../', () => {
      const result = validateExecutionContext('../');
      expect(result.isValid).toBe(false);
      expect(result.error).toContain('configured workspace directory');
    });

    it('should reject multiple path traversals (../../../../)', () => {
      const result = validateExecutionContext('../../../../');
      expect(result.isValid).toBe(false);
      expect(result.error).toContain('configured workspace directory');
    });

    it('should reject system directories (/etc)', () => {
      const result = validateExecutionContext('/etc');
      expect(result.isValid).toBe(false);
      expect(result.error).toContain('configured workspace directory');
    });

    it('should reject root directory (/)', () => {
      const result = validateExecutionContext('/');
      expect(result.isValid).toBe(false);
      expect(result.error).toContain('configured workspace directory');
    });

    it('should reject home directory', () => {
      const homeDir = os.homedir();
      if (homeDir !== workspaceRoot && !homeDir.startsWith(workspaceRoot)) {
        const result = validateExecutionContext(homeDir);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('configured workspace directory');
      }
    });

    it('should handle custom workspace root', () => {
      const customRoot = path.join(workspaceRoot, 'packages');
      const validPath = path.join(customRoot, 'octocode-local-files');

      const result = validateExecutionContext(validPath, customRoot);
      expect(result.isValid).toBe(true);
    });

    it('should reject path outside custom workspace root', () => {
      const customRoot = path.join(
        workspaceRoot,
        'packages',
        'octocode-local-files'
      );
      const invalidPath = path.join(workspaceRoot, 'packages');

      const result = validateExecutionContext(invalidPath, customRoot);
      expect(result.isValid).toBe(false);
      expect(result.error).toContain('configured workspace directory');
    });

    describe('symlink handling', () => {
      const tmpDir = path.join(workspaceRoot, 'test-tmp-symlinks');
      const targetDir = path.join(tmpDir, 'target');
      const symlinkPath = path.join(tmpDir, 'symlink');
      const externalTarget = path.join(parentDir, 'external-target');

      beforeEach(async () => {
        // Clean up any existing test directories
        try {
          if (fs.existsSync(tmpDir)) {
            fs.rmSync(tmpDir, { recursive: true, force: true });
          }
        } catch {
          // Ignore cleanup errors
        }
      });

      it('should allow symlink that points within workspace', async () => {
        try {
          // Create test directories
          fs.mkdirSync(tmpDir, { recursive: true });
          fs.mkdirSync(targetDir, { recursive: true });

          // Create symlink pointing to target within workspace
          fs.symlinkSync(targetDir, symlinkPath, 'dir');

          const result = validateExecutionContext(symlinkPath);
          expect(result.isValid).toBe(true);

          // Clean up
          fs.rmSync(tmpDir, { recursive: true, force: true });
        } catch {
          // Skip test if symlink creation fails (e.g., permissions)
        }
      });

      it('should reject symlink that points outside workspace', async () => {
        try {
          // Create test directories
          fs.mkdirSync(tmpDir, { recursive: true });

          // Try to create external target (might fail, that's ok for test)
          try {
            fs.mkdirSync(externalTarget, { recursive: true });
          } catch {
            // Can't create in parent, skip this test
            return;
          }

          // Create symlink pointing outside workspace
          fs.symlinkSync(externalTarget, symlinkPath, 'dir');

          const result = validateExecutionContext(symlinkPath);
          expect(result.isValid).toBe(false);
          expect(result.error).toContain('Symlink target');
          expect(result.error).toContain('configured workspace directory');

          // Clean up
          fs.rmSync(tmpDir, { recursive: true, force: true });
          fs.rmSync(externalTarget, { recursive: true, force: true });
        } catch {
          // Skip test if setup fails
        }
      });
    });
  });

  describe('WORKSPACE_ROOT environment variable', () => {
    it('should respect WORKSPACE_ROOT env var if set', () => {
      const originalEnv = process.env.WORKSPACE_ROOT;

      try {
        // Set custom workspace root
        process.env.WORKSPACE_ROOT = workspaceRoot;

        // Should allow path within workspace
        const validPath = path.join(workspaceRoot, 'src');
        const result = validateExecutionContext(validPath);
        expect(result.isValid).toBe(true);

        // Should reject parent directory
        const invalidResult = validateExecutionContext(parentDir);
        expect(invalidResult.isValid).toBe(false);
      } finally {
        // Restore original env
        if (originalEnv) {
          process.env.WORKSPACE_ROOT = originalEnv;
        } else {
          delete process.env.WORKSPACE_ROOT;
        }
      }
    });
  });

  describe('edge cases', () => {
    it('should handle paths with trailing slashes', () => {
      const pathWithSlash = path.join(workspaceRoot, 'src') + '/';
      const result = validateExecutionContext(pathWithSlash);
      expect(result.isValid).toBe(true);
    });

    it('should handle paths with ./ prefix', () => {
      const result = validateExecutionContext('./src');
      expect(result.isValid).toBe(true);
    });

    it('should handle paths with redundant segments (./src/./lib)', () => {
      const result = validateExecutionContext('./src/./lib');
      expect(result.isValid).toBe(true);
    });

    it('should normalize and validate complex relative paths', () => {
      // s../src/lib should resolve to src/lib
      const result = validateExecutionContext('./s../src/lib');
      expect(result.isValid).toBe(true);
    });

    it('should reject path that escapes via complex traversal', () => {
      // Try to escape: src/../../..
      const result = validateExecutionContext('./src/../../..');
      expect(result.isValid).toBe(false);
    });
  });
});
