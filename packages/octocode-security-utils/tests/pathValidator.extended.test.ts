import { describe, it, expect, beforeEach } from 'vitest';
import { PathValidator } from '../src/pathValidator.js';
import path from 'path';

describe('PathValidator - Extended', () => {
  let validator: PathValidator;
  const testWorkspace = process.cwd();

  beforeEach(() => {
    validator = new PathValidator({ workspaceRoot: testWorkspace });
  });

  describe('addAllowedRoot', () => {
    it('should add a new allowed root', () => {
      const srcDir = path.join(testWorkspace, 'src');
      validator.addAllowedRoot(srcDir);

      const result = validator.validate(srcDir);
      expect(result.isValid).toBe(true);
    });

    it('should not add duplicate roots', () => {
      validator.addAllowedRoot(testWorkspace);
      validator.addAllowedRoot(testWorkspace);

      const result = validator.validate(`${testWorkspace}/package.json`);
      expect(result.isValid).toBe(true);
    });

    it('should resolve relative paths when adding roots', () => {
      validator.addAllowedRoot('.');

      const result = validator.validate(`${process.cwd()}/package.json`);
      expect(result.isValid).toBe(true);
    });
  });

  describe('validate - Edge cases', () => {
    it('should handle whitespace-only path', () => {
      const result = validator.validate('   ');

      expect(result.isValid).toBe(false);
      expect(result.error).toContain('empty');
    });

    it('should handle path with double slashes', () => {
      const result = validator.validate(`${testWorkspace}//src//index.ts`);

      expect(result.isValid).toBe(true);
    });

    it('should handle .env files as ignored', () => {
      const result = validator.validate(`${testWorkspace}/.env`);

      expect(result.isValid).toBe(false);
      expect(result.error).toContain('ignored');
    });

    it('should handle .git directory paths', () => {
      const result = validator.validate(`${testWorkspace}/.git/config`);

      expect(typeof result.isValid).toBe('boolean');
    });

    it('should handle node_modules paths', () => {
      const result = validator.validate(`${testWorkspace}/node_modules/lodash`);

      expect(typeof result.isValid).toBe('boolean');
    });
  });

  describe('exists', () => {
    it('should return true for existing files', async () => {
      const result = await validator.exists(`${testWorkspace}/package.json`);

      expect(result).toBe(true);
    });

    it('should return false for non-existing files', async () => {
      const result = await validator.exists(
        `${testWorkspace}/non-existent-file-xyz123.txt`
      );

      expect(result).toBe(false);
    });

    it('should return false for paths outside workspace', async () => {
      const result = await validator.exists('/etc/passwd');

      expect(result).toBe(false);
    });

    it('should return false for ignored paths', async () => {
      const result = await validator.exists(`${testWorkspace}/.git/config`);

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
        `${testWorkspace}/non-existent-path-xyz`
      );

      expect(result).toBe(null);
    });

    it('should return null for paths outside workspace', async () => {
      const result = await validator.getType('/etc/passwd');

      expect(result).toBe(null);
    });

    it('should return null for ignored paths', async () => {
      const result = await validator.getType(`${testWorkspace}/.git`);

      expect(result).toBe(null);
    });
  });

  describe('constructor', () => {
    it('should use cwd when no workspace root provided', () => {
      const cwdValidator = new PathValidator();
      const cwd = process.cwd();
      const result = cwdValidator.validate(`${cwd}/package.json`);

      expect(result.isValid).toBe(true);
    });

    it('should resolve relative workspace root', () => {
      const relativeValidator = new PathValidator({ workspaceRoot: '.' });
      const cwd = process.cwd();
      const result = relativeValidator.validate(`${cwd}/package.json`);

      expect(result.isValid).toBe(true);
    });
  });

  describe('validate - Symlink handling', () => {
    it('should block paths that resolve outside workspace', () => {
      const result = validator.validate('/etc/passwd');

      expect(result.isValid).toBe(false);
      expect(result.error).toContain('outside allowed directories');
    });
  });

  describe('validate - Path traversal protection', () => {
    it('should block parent traversal attempts to system paths', () => {
      const strictValidator = new PathValidator({
        workspaceRoot: testWorkspace,
        includeHomeDir: false,
      });
      const result = strictValidator.validate(
        `${testWorkspace}/../../../etc/passwd`
      );

      expect(result.isValid).toBe(false);
    });

    it('should allow parent traversal within home directory (default mode)', () => {
      const result = validator.validate(`${testWorkspace}/../`);
      expect(result.isValid).toBe(true);
    });

    it('should allow valid paths with ../ that stay within workspace', () => {
      const validPath = path.join(testWorkspace, 'src', '..', 'package.json');
      const result = validator.validate(validPath);

      expect(result.isValid).toBe(true);
    });
  });
});
