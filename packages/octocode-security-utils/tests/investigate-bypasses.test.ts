/**
 * Investigating potential security bypasses
 */

import { describe, it, expect } from 'vitest';
import { PathValidator } from '../src/pathValidator.js';
import path from 'path';
import { execSync } from 'child_process';

describe('🔍 Investigating Potential Bypasses', () => {
  // Use cwd as workspace to ensure tests work in CI
  const workspaceRoot = process.cwd();
  const validator = new PathValidator({ workspaceRoot });

  describe('URL Encoding Analysis', () => {
    it('URL encoded %2e%2e - check if real bypass', () => {
      const testPath = path.join(workspaceRoot, '%2e%2e/%2e%2e/etc');
      validator.validate(testPath);

      // What does Node's path.resolve do?
      const resolved = path.resolve(testPath);

      const escapedWorkspace = !resolved.startsWith(workspaceRoot);

      // This test should actually pass if it doesn't escape
      expect(escapedWorkspace).toBe(false);
    });

    it('Double URL encoding %252e - check if real bypass', () => {
      const testPath = path.join(workspaceRoot, '%252e%252e/etc');
      validator.validate(testPath);
      const resolved = path.resolve(testPath);

      expect(resolved.startsWith(workspaceRoot)).toBe(true);
    });
  });

  describe('Unicode Analysis', () => {
    it('Full-width dots ．． - check if real bypass', () => {
      const testPath = path.join(workspaceRoot, '．．/etc');
      validator.validate(testPath);
      const resolved = path.resolve(testPath);

      expect(resolved.startsWith(workspaceRoot)).toBe(true);
    });
  });

  describe('Real-World Bypass Test', () => {
    it('Can URL encoding bypass actual file system operations?', () => {
      try {
        execSync('ls /Users/%2e%2e 2>&1', { encoding: 'utf-8', timeout: 1000 });
        expect.fail('Shell command should have failed');
      } catch {
        // Expected: command fails (e.g. No such file) - %2e%2e treated as literal
      }
    });
  });

  describe('Can MCP Tools Bypass with Encoding?', () => {
    it.skip('Test if encoded paths work through actual command execution (requires octocode-mcp safeExec)', async () => {
      const { safeExec } = await import('../src/utils/exec/safe.js');

      try {
        await safeExec('ls', ['/Users/%2e%2e']);
        expect.fail('Command should have failed - potential bypass!');
      } catch {
        // Expected: command fails - path is rejected
      }
    });
  });
});
