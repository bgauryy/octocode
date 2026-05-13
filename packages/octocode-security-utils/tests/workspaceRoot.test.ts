/**
 * Tests for resolveWorkspaceRoot - unified workspace root resolution.
 *
 * Priority chain:
 *   1. Explicit parameter
 *   2. WORKSPACE_ROOT env var (existing directory only)
 *   3. process.cwd() fallback
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import path from 'path';

import { resolveWorkspaceRoot } from '../src/workspaceRoot.js';

describe('resolveWorkspaceRoot', () => {
  const cwd = process.cwd();
  let savedEnv: NodeJS.ProcessEnv;

  beforeEach(() => {
    savedEnv = { ...process.env };
    delete process.env.WORKSPACE_ROOT;
  });

  afterEach(() => {
    process.env = savedEnv;
  });

  describe('priority chain', () => {
    it('explicit parameter wins over env var', () => {
      const explicit = path.join(cwd, 'explicit-root');
      process.env.WORKSPACE_ROOT = cwd;
      expect(resolveWorkspaceRoot(explicit)).toBe(path.resolve(explicit));
    });

    it('env var wins when no explicit parameter', () => {
      process.env.WORKSPACE_ROOT = cwd;
      expect(resolveWorkspaceRoot()).toBe(path.resolve(cwd));
    });

    it('falls back to process.cwd() when nothing is set', () => {
      expect(resolveWorkspaceRoot()).toBe(cwd);
    });
  });

  describe('explicit parameter', () => {
    it('should resolve absolute path as-is', () => {
      expect(resolveWorkspaceRoot('/usr/local/workspace')).toBe(
        path.resolve('/usr/local/workspace')
      );
    });

    it('should resolve relative path against cwd', () => {
      expect(resolveWorkspaceRoot('./my-project')).toBe(
        path.resolve('./my-project')
      );
    });

    it('should resolve paths with ../', () => {
      expect(resolveWorkspaceRoot('../sibling-project')).toBe(
        path.resolve('../sibling-project')
      );
    });
  });

  describe('WORKSPACE_ROOT env var', () => {
    it('should resolve absolute env path', () => {
      process.env.WORKSPACE_ROOT = cwd;
      expect(resolveWorkspaceRoot()).toBe(path.resolve(cwd));
    });

    it('should ignore relative env path when target does not exist', () => {
      process.env.WORKSPACE_ROOT = './relative-workspace';
      expect(resolveWorkspaceRoot()).toBe(cwd);
    });

    it('should trim whitespace from env value', () => {
      process.env.WORKSPACE_ROOT = `  ${cwd}  `;
      expect(resolveWorkspaceRoot()).toBe(path.resolve(cwd));
    });

    it('should ignore empty string env value (treat as unset)', () => {
      process.env.WORKSPACE_ROOT = '';
      expect(resolveWorkspaceRoot()).toBe(cwd);
    });

    it('should ignore whitespace-only env value (treat as unset)', () => {
      process.env.WORKSPACE_ROOT = '   ';
      expect(resolveWorkspaceRoot()).toBe(cwd);
    });

    it('should ignore non-existent WORKSPACE_ROOT and fall back to cwd', () => {
      process.env.WORKSPACE_ROOT = path.join(cwd, '__does_not_exist__');
      expect(resolveWorkspaceRoot()).toBe(cwd);
    });
  });

  describe('return value properties', () => {
    it('should always return an absolute path', () => {
      expect(path.isAbsolute(resolveWorkspaceRoot())).toBe(true);
    });

    it('should return resolved (normalized) path from explicit', () => {
      const result = resolveWorkspaceRoot('/foo/bar/../baz');
      expect(result).toBe(path.resolve('/foo/bar/../baz'));
      expect(result).not.toContain('..');
    });

    it('should return resolved path from env', () => {
      process.env.WORKSPACE_ROOT = path.join(cwd, 'packages', '..');
      expect(resolveWorkspaceRoot()).toBe(path.resolve(cwd));
    });

    it('should return a non-empty string', () => {
      const result = resolveWorkspaceRoot();
      expect(typeof result).toBe('string');
      expect(result.length).toBeGreaterThan(0);
    });

    it('should be idempotent (same result on repeated calls)', () => {
      expect(resolveWorkspaceRoot()).toBe(resolveWorkspaceRoot());
    });
  });

  describe('security - path traversal via env', () => {
    it('should ignore traversal env root when resolved path does not exist', () => {
      process.env.WORKSPACE_ROOT = path.join(cwd, '..', '..', '__missing__');
      expect(resolveWorkspaceRoot()).toBe(cwd);
    });

    it('should resolve traversal attempts in explicit param', () => {
      const result = resolveWorkspaceRoot('/opt/workspace/../../etc');
      expect(result).toBe(path.resolve('/etc'));
      expect(result).not.toContain('..');
    });
  });
});
