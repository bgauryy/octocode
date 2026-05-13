/**
 * Tests for extra allowed roots via SecurityRegistry.
 *
 * Verifies that PathValidator and executionContextValidator respect
 * roots registered through securityRegistry.addAllowedRoots(),
 * which is how octocode-mcp (and any other consumer) grants access
 * to app-specific directories like ~/.octocode/repos/.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import path from 'path';
import os from 'os';

import { PathValidator } from '../src/pathValidator.js';
import { validateExecutionContext } from '../src/executionContextValidator.js';
import { securityRegistry } from '../src/registry.js';

describe('extra allowed roots via SecurityRegistry', () => {
  const MOCK_APP_HOME = path.join(os.homedir(), '.myapp-test-mock');

  beforeEach(() => {
    securityRegistry.reset();
    securityRegistry.addAllowedRoots([MOCK_APP_HOME]);
  });

  afterEach(() => {
    securityRegistry.reset();
  });

  const repoPath = path.join(
    MOCK_APP_HOME,
    'repos',
    'facebook',
    'react',
    'main'
  );
  const filePath = path.join(repoPath, 'src', 'React.ts');

  describe('PathValidator includes registry roots', () => {
    it('should include registered root in allowed roots', () => {
      const validator = new PathValidator({ workspaceRoot: '/tmp/myproject' });
      expect(validator.getAllowedRoots()).toContain(
        path.resolve(MOCK_APP_HOME)
      );
    });

    it('should allow paths under registered root (ENOENT → isValid)', () => {
      const validator = new PathValidator({
        workspaceRoot: '/tmp/myproject',
        includeHomeDir: false,
      });
      expect(validator.validate(repoPath).isValid).toBe(true);
    });

    it('should allow deep file paths under registered root', () => {
      const validator = new PathValidator({
        workspaceRoot: '/tmp/myproject',
        includeHomeDir: false,
      });
      expect(validator.validate(filePath).isValid).toBe(true);
    });

    it('should not duplicate root when registered twice', () => {
      securityRegistry.addAllowedRoots([MOCK_APP_HOME]);
      const validator = new PathValidator({
        workspaceRoot: '/tmp/myproject',
      });
      const roots = validator.getAllowedRoots();
      const count = roots.filter(r => r === path.resolve(MOCK_APP_HOME)).length;
      expect(count).toBe(1);
    });
  });

  describe('executionContextValidator allows registry roots', () => {
    it('should allow cwd inside registered root', () => {
      expect(validateExecutionContext(repoPath)).toMatchObject({
        isValid: true,
      });
    });

    it('should allow cwd nested under a registered root', () => {
      const nestedPath = path.join(repoPath, 'packages', 'core');
      expect(validateExecutionContext(nestedPath)).toMatchObject({
        isValid: true,
      });
    });

    it('should allow cwd at the registered root itself', () => {
      expect(validateExecutionContext(MOCK_APP_HOME)).toMatchObject({
        isValid: true,
      });
    });
  });

  describe('without registered roots', () => {
    beforeEach(() => {
      securityRegistry.reset();
    });

    it('PathValidator should NOT include app home by default', () => {
      const validator = new PathValidator({
        workspaceRoot: '/tmp/myproject',
        includeHomeDir: false,
      });
      expect(validator.getAllowedRoots()).not.toContain(
        path.resolve(MOCK_APP_HOME)
      );
    });
  });
});
