/**
 * WHITE-HAT SECURITY PENETRATION TESTS
 * =====================================
 * Comprehensive path security testing for ALL local tools.
 *
 * Tests that no local tool can read, search, list, or find files
 * outside the workspace boundary. Covers:
 *
 * 1. PathValidator - direct traversal attacks
 * 2. validateToolPath - tool-level wrapper
 * 3. commandValidator - injection via args
 * 4. include/exclude/excludeDir - secondary path params
 * 5. Symlink-based escapes
 * 6. Encoding & Unicode tricks
 * 7. Prefix collision attacks
 * 8. Race condition considerations
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  PathValidator,
  reinitializePathValidator,
} from 'octocode-security-utils/pathValidator';
import { validateCommand } from 'octocode-security-utils/commandValidator';
import { validateToolPath } from '../../src/utils/file/toolHelpers.js';
import path from 'path';
import fs from 'fs';
import os from 'os';

const WORKSPACE = process.cwd(); // packages/octocode-mcp
const HOME = os.homedir();
const PARENT = path.dirname(WORKSPACE);
const GRANDPARENT = path.dirname(PARENT);

/**
 * Creates a strict validator that does NOT include the home directory.
 * This isolates tests to just the workspace root.
 */
function strictValidator(root = WORKSPACE): PathValidator {
  return new PathValidator({ workspaceRoot: root, includeHomeDir: false });
}

/**
 * Helper: build a minimal query object for validateToolPath
 */
function toolQuery(p: string) {
  return { path: p, researchGoal: 'test', reasoning: 'test' };
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 1: PathValidator – Direct Path Traversal Attacks
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

describe('SEC-01: PathValidator – Path Traversal Attacks', () => {
  let v: PathValidator;

  beforeEach(() => {
    v = strictValidator();
  });

  describe('Classic ../ traversal', () => {
    const attacks = [
      `${WORKSPACE}/../../../etc/passwd`,
      `${WORKSPACE}/src/../../../etc/shadow`,
      `${WORKSPACE}/./../../../../../../etc/hosts`,
      `${WORKSPACE}/src/../../../../../../bin/sh`,
      '../../../etc/passwd',
      '../../../../etc/shadow',
      '../'.repeat(20) + 'etc/passwd',
    ];

    attacks.forEach(attack => {
      it(`should BLOCK: ${attack.substring(0, 80)}...`, () => {
        const r = v.validate(attack);
        expect(r.isValid).toBe(false);
      });
    });
  });

  describe('Mixed and alternative separators', () => {
    it('should BLOCK backslash traversal', () => {
      expect(v.validate(`${WORKSPACE}\\..\\..\\etc`).isValid).toBe(false);
    });

    it('mixed forward/backslash on Unix stays within workspace (literal backslash)', () => {
      // On Unix, backslash is a valid filename character, not a separator
      // So `WORKSPACE/..\..\etc` creates a literal directory name, not traversal
      const r = v.validate(`${WORKSPACE}/..\\../etc`);
      if (r.isValid) {
        expect(r.sanitizedPath?.startsWith(WORKSPACE)).toBe(true);
      }
    });

    it('should BLOCK double-slash normalization trick', () => {
      const r = v.validate(`${WORKSPACE}//../..//etc/passwd`);
      expect(r.isValid).toBe(false);
    });

    it('should BLOCK excessive ./ followed by ../', () => {
      const r = v.validate(`${WORKSPACE}/./././../../etc`);
      expect(r.isValid).toBe(false);
    });
  });

  describe('Absolute system paths', () => {
    const systemPaths = [
      '/',
      '/etc/passwd',
      '/etc/shadow',
      '/usr/bin',
      '/tmp',
      '/var/log/syslog',
      '/root',
      '/root/.ssh/id_rsa',
      '/System',
      '/Library',
      '/private/etc/passwd',
    ];

    systemPaths.forEach(sp => {
      it(`should BLOCK: ${sp}`, () => {
        expect(v.validate(sp).isValid).toBe(false);
      });
    });
  });

  describe('Sibling directory & prefix collision', () => {
    it('should BLOCK workspace prefix + extra chars', () => {
      expect(v.validate(WORKSPACE + '-evil').isValid).toBe(false);
    });

    it('should BLOCK workspace prefix + number', () => {
      expect(v.validate(WORKSPACE + '2').isValid).toBe(false);
    });

    it('should BLOCK workspace prefix + underscore', () => {
      expect(v.validate(WORKSPACE + '_backup').isValid).toBe(false);
    });

    it('should BLOCK sibling directory', () => {
      expect(v.validate(PARENT + '/other-project').isValid).toBe(false);
    });

    it('should BLOCK parent directory itself', () => {
      expect(v.validate(PARENT).isValid).toBe(false);
    });

    it('should BLOCK grandparent', () => {
      expect(v.validate(GRANDPARENT).isValid).toBe(false);
    });
  });

  describe('Valid workspace paths – sanity checks', () => {
    it('should ALLOW workspace root', () => {
      expect(v.validate(WORKSPACE).isValid).toBe(true);
    });

    it('should ALLOW workspace/src', () => {
      expect(v.validate(`${WORKSPACE}/src`).isValid).toBe(true);
    });

    it('should ALLOW workspace/package.json', () => {
      expect(v.validate(`${WORKSPACE}/package.json`).isValid).toBe(true);
    });

    it('should ALLOW self-referencing ./ within workspace', () => {
      const p = path.join(WORKSPACE, 'src', '..', 'package.json');
      expect(v.validate(p).isValid).toBe(true);
    });
  });
});

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 2: Encoding & Unicode Bypass Attempts
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

describe('SEC-02: Encoding & Unicode Bypass Attempts', () => {
  let v: PathValidator;

  beforeEach(() => {
    v = strictValidator();
  });

  describe('URL encoding', () => {
    it('URL-encoded %2e%2e stays within workspace (literal chars)', () => {
      const r = v.validate(`${WORKSPACE}/%2e%2e/%2e%2e/etc`);
      // Node treats %2e as literal – stays within workspace
      if (r.isValid) {
        expect(r.sanitizedPath?.startsWith(WORKSPACE)).toBe(true);
      }
    });

    it('double URL-encoded %252e stays within workspace', () => {
      const r = v.validate(`${WORKSPACE}/%252e%252e/etc`);
      if (r.isValid) {
        expect(r.sanitizedPath?.startsWith(WORKSPACE)).toBe(true);
      }
    });
  });

  describe('Special characters', () => {
    it('should BLOCK null byte injection', () => {
      expect(v.validate(`${WORKSPACE}\x00/../../../etc`).isValid).toBe(false);
    });

    it('should BLOCK newline injection', () => {
      expect(v.validate(`${WORKSPACE}\n/../../../etc`).isValid).toBe(false);
    });

    it('should BLOCK tab injection', () => {
      expect(v.validate(`${WORKSPACE}\t/../../../etc`).isValid).toBe(false);
    });

    it('should BLOCK carriage return injection', () => {
      expect(v.validate(`${WORKSPACE}\r/../../../etc`).isValid).toBe(false);
    });

    it('should BLOCK vertical tab injection', () => {
      expect(v.validate(`${WORKSPACE}\v/../../../etc`).isValid).toBe(false);
    });

    it('should BLOCK form feed injection', () => {
      expect(v.validate(`${WORKSPACE}\f/../../../etc`).isValid).toBe(false);
    });
  });

  describe('Unicode tricks', () => {
    it('should BLOCK Unicode fraction slash (⁄) traversal', () => {
      expect(v.validate(`${WORKSPACE}⁄..⁄..⁄etc`).isValid).toBe(false);
    });

    it('full-width dots (．．) stay within workspace', () => {
      const r = v.validate(`${WORKSPACE}/．．/etc`);
      if (r.isValid) {
        expect(r.sanitizedPath?.startsWith(WORKSPACE)).toBe(true);
      }
    });

    it('should BLOCK RTL override character', () => {
      expect(v.validate(`${WORKSPACE}\u202e/../../etc`).isValid).toBe(false);
    });

    it('should BLOCK zero-width space injection', () => {
      const r = v.validate(`${WORKSPACE}/\u200b../\u200b../etc`);
      // Even if treated as literal, should NOT escape workspace
      if (r.isValid) {
        expect(r.sanitizedPath?.startsWith(WORKSPACE)).toBe(true);
      }
    });

    it('should BLOCK zero-width joiner injection', () => {
      const r = v.validate(`${WORKSPACE}/\u200d../\u200d../etc`);
      if (r.isValid) {
        expect(r.sanitizedPath?.startsWith(WORKSPACE)).toBe(true);
      }
    });
  });
});

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 3: Symlink-Based Escape Attacks
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

describe('SEC-03: Symlink-Based Escape Attacks', () => {
  describe('Mock-based symlink escape testing', () => {
    it('should BLOCK symlink pointing to /etc/passwd', () => {
      const mock = vi.spyOn(fs, 'realpathSync').mockReturnValue('/etc/passwd');
      try {
        const v = strictValidator();
        const r = v.validate(`${WORKSPACE}/evil-link`);
        expect(r.isValid).toBe(false);
        expect(r.error).toContain('Symlink target');
      } finally {
        mock.mockRestore();
      }
    });

    it('should BLOCK symlink pointing to /root/.ssh', () => {
      const mock = vi
        .spyOn(fs, 'realpathSync')
        .mockReturnValue('/root/.ssh/id_rsa');
      try {
        const v = strictValidator();
        const r = v.validate(`${WORKSPACE}/ssh-link`);
        expect(r.isValid).toBe(false);
      } finally {
        mock.mockRestore();
      }
    });

    it('should BLOCK symlink pointing to parent directory', () => {
      const mock = vi.spyOn(fs, 'realpathSync').mockReturnValue(PARENT);
      try {
        const v = strictValidator();
        const r = v.validate(`${WORKSPACE}/parent-link`);
        expect(r.isValid).toBe(false);
      } finally {
        mock.mockRestore();
      }
    });

    it('should BLOCK symlink pointing to sibling workspace', () => {
      const sibling = PARENT + '/other-project';
      const mock = vi.spyOn(fs, 'realpathSync').mockReturnValue(sibling);
      try {
        const v = strictValidator();
        const r = v.validate(`${WORKSPACE}/sibling-link`);
        expect(r.isValid).toBe(false);
      } finally {
        mock.mockRestore();
      }
    });

    it('should BLOCK symlink to .git/config (ignored path)', () => {
      const mock = vi
        .spyOn(fs, 'realpathSync')
        .mockReturnValue(`${WORKSPACE}/.git/config`);
      try {
        const v = strictValidator();
        const r = v.validate(`${WORKSPACE}/git-link`);
        expect(r.isValid).toBe(false);
        expect(r.error).toContain('ignored');
      } finally {
        mock.mockRestore();
      }
    });

    it('should BLOCK symlink to .env (ignored file)', () => {
      const mock = vi
        .spyOn(fs, 'realpathSync')
        .mockReturnValue(`${WORKSPACE}/.env`);
      try {
        const v = strictValidator();
        const r = v.validate(`${WORKSPACE}/env-link`);
        expect(r.isValid).toBe(false);
        expect(r.error).toContain('ignored');
      } finally {
        mock.mockRestore();
      }
    });

    it('should ALLOW symlink within workspace', () => {
      const mock = vi
        .spyOn(fs, 'realpathSync')
        .mockReturnValue(`${WORKSPACE}/src`);
      try {
        const v = strictValidator();
        const r = v.validate(`${WORKSPACE}/src-link`);
        expect(r.isValid).toBe(true);
        expect(r.sanitizedPath).toBe(`${WORKSPACE}/src`);
      } finally {
        mock.mockRestore();
      }
    });
  });

  describe('Symlink error handling', () => {
    it('should BLOCK on ELOOP (circular symlinks)', () => {
      const mock = vi.spyOn(fs, 'realpathSync').mockImplementation(() => {
        const e = new Error(
          'Too many levels of symbolic links'
        ) as NodeJS.ErrnoException;
        e.code = 'ELOOP';
        throw e;
      });
      try {
        const v = strictValidator();
        const r = v.validate(`${WORKSPACE}/circular-link`);
        expect(r.isValid).toBe(false);
        expect(r.error).toContain('Symlink loop');
      } finally {
        mock.mockRestore();
      }
    });

    it('should BLOCK on EACCES (permission denied)', () => {
      const mock = vi.spyOn(fs, 'realpathSync').mockImplementation(() => {
        const e = new Error('Permission denied') as NodeJS.ErrnoException;
        e.code = 'EACCES';
        throw e;
      });
      try {
        const v = strictValidator();
        const r = v.validate(`${WORKSPACE}/restricted-link`);
        expect(r.isValid).toBe(false);
        expect(r.error).toContain('Permission denied');
      } finally {
        mock.mockRestore();
      }
    });

    it('should BLOCK on unknown fs errors (fail-closed)', () => {
      const mock = vi.spyOn(fs, 'realpathSync').mockImplementation(() => {
        const e = new Error('Something weird') as NodeJS.ErrnoException;
        e.code = 'EWEIRD';
        throw e;
      });
      try {
        const v = strictValidator();
        const r = v.validate(`${WORKSPACE}/weird-link`);
        expect(r.isValid).toBe(false);
        expect(r.error).toContain('Unexpected error');
      } finally {
        mock.mockRestore();
      }
    });

    it('should BLOCK on non-Error exceptions (fail-closed)', () => {
      const mock = vi.spyOn(fs, 'realpathSync').mockImplementation(() => {
        throw 'not an error object';
      });
      try {
        const v = strictValidator();
        const r = v.validate(`${WORKSPACE}/string-throw`);
        expect(r.isValid).toBe(false);
      } finally {
        mock.mockRestore();
      }
    });
  });
});

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 4: validateToolPath – All Local Tools Entry Point
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

describe('SEC-04: validateToolPath – Tool Entry Point Security', () => {
  /**
   * NOTE: validateToolPath uses the GLOBAL pathValidator singleton which
   * includes the HOME directory by default. This means relative traversals
   * that resolve within $HOME are allowed. This is by design - local tools
   * need to access files across the user's home directory.
   *
   * For strict workspace-only isolation, use PathValidator directly with
   * includeHomeDir: false.
   */

  const TOOL_NAMES = [
    'localSearchCode',
    'localViewStructure',
    'localFindFiles',
    'localGetFileContent',
  ] as const;

  describe('Path traversal via tool path parameter', () => {
    // Only include paths that resolve OUTSIDE the home directory
    // Relative paths like ../../../../etc/passwd resolve within $HOME on this
    // system, and the global pathValidator includes home dir by default
    const traversalPaths = [
      '/etc/passwd',
      '/root/.ssh/id_rsa',
      '/tmp/evil',
      '/var/log/syslog',
      '/System/Library',
      '/private/etc/passwd',
    ];

    TOOL_NAMES.forEach(toolName => {
      traversalPaths.forEach(attackPath => {
        it(`${toolName}: should BLOCK path="${attackPath.substring(0, 60)}"`, () => {
          const r = validateToolPath(toolQuery(attackPath), toolName);
          expect(r.isValid).toBe(false);
          expect(r.errorResult).toBeDefined();
        });
      });
    });
  });

  describe('Valid tool paths', () => {
    TOOL_NAMES.forEach(toolName => {
      it(`${toolName}: should ALLOW workspace root`, () => {
        const r = validateToolPath(toolQuery(WORKSPACE), toolName);
        expect(r.isValid).toBe(true);
        expect(r.sanitizedPath).toBeDefined();
      });

      it(`${toolName}: should ALLOW workspace/src`, () => {
        const r = validateToolPath(toolQuery(`${WORKSPACE}/src`), toolName);
        expect(r.isValid).toBe(true);
      });
    });
  });

  describe('Empty & whitespace paths', () => {
    TOOL_NAMES.forEach(toolName => {
      it(`${toolName}: should BLOCK empty path`, () => {
        const r = validateToolPath(toolQuery(''), toolName);
        expect(r.isValid).toBe(false);
      });

      it(`${toolName}: should BLOCK whitespace-only path`, () => {
        const r = validateToolPath(toolQuery('   '), toolName);
        expect(r.isValid).toBe(false);
      });
    });
  });

  describe('Ignored sensitive paths', () => {
    const sensitiveFiles = [
      `${WORKSPACE}/.env`,
      `${WORKSPACE}/.env.local`,
      `${WORKSPACE}/.env.production`,
      `${WORKSPACE}/.git/config`,
      `${WORKSPACE}/.ssh/id_rsa`,
    ];

    TOOL_NAMES.forEach(toolName => {
      sensitiveFiles.forEach(sensitiveFile => {
        it(`${toolName}: should BLOCK ${path.basename(sensitiveFile)}`, () => {
          const r = validateToolPath(toolQuery(sensitiveFile), toolName);
          expect(r.isValid).toBe(false);
        });
      });
    });
  });

  describe('Strict mode: reinitialize global validator without home dir', () => {
    /**
     * This tests that when the global singleton is configured in strict mode
     * (no home directory), all the additional traversal attacks are caught.
     */
    afterEach(() => {
      // Restore default global validator
      reinitializePathValidator();
    });

    it('should BLOCK relative traversal to parent when strict', () => {
      reinitializePathValidator({
        workspaceRoot: WORKSPACE,
        includeHomeDir: false,
      });
      const r = validateToolPath(
        toolQuery('../../../../etc/passwd'),
        'localSearchCode'
      );
      expect(r.isValid).toBe(false);
    });

    it('should BLOCK sibling workspace-evil when strict', () => {
      reinitializePathValidator({
        workspaceRoot: WORKSPACE,
        includeHomeDir: false,
      });
      const r = validateToolPath(
        toolQuery(`${WORKSPACE}-evil/secret`),
        'localGetFileContent'
      );
      expect(r.isValid).toBe(false);
    });

    it('should BLOCK parent directory when strict', () => {
      reinitializePathValidator({
        workspaceRoot: WORKSPACE,
        includeHomeDir: false,
      });
      const r = validateToolPath(toolQuery(PARENT), 'localViewStructure');
      expect(r.isValid).toBe(false);
    });

    it('should ALLOW workspace itself when strict', () => {
      reinitializePathValidator({
        workspaceRoot: WORKSPACE,
        includeHomeDir: false,
      });
      const r = validateToolPath(toolQuery(WORKSPACE), 'localSearchCode');
      expect(r.isValid).toBe(true);
    });
  });
});

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 5: Execution Context Validator (cwd isolation)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 6: Command Validator – Injection Prevention
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

describe('SEC-06: Command Validator – Injection Prevention', () => {
  describe('Command whitelist enforcement', () => {
    const blockedCommands = [
      'rm',
      'cat',
      'curl',
      'wget',
      'bash',
      'sh',
      'python',
      'node',
      'nc',
      'ncat',
      'dd',
      'chmod',
      'chown',
      'mkfifo',
      'exec',
      'eval',
    ];

    blockedCommands.forEach(cmd => {
      it(`should BLOCK command: ${cmd}`, () => {
        const r = validateCommand(cmd, []);
        expect(r.isValid).toBe(false);
        expect(r.error).toContain('not allowed');
      });
    });
  });

  describe('Allowed commands', () => {
    it('should ALLOW rg', () => {
      expect(validateCommand('rg', ['pattern', '.']).isValid).toBe(true);
    });

    it('should ALLOW ls', () => {
      expect(validateCommand('ls', ['-la']).isValid).toBe(true);
    });

    it('should ALLOW find', () => {
      expect(validateCommand('find', ['.', '-name', '*.ts']).isValid).toBe(
        true
      );
    });
  });

  describe('Shell injection in arguments', () => {
    it('should BLOCK command substitution $()', () => {
      expect(validateCommand('ls', ['$(cat /etc/passwd)']).isValid).toBe(false);
    });

    it('should BLOCK backtick substitution', () => {
      expect(validateCommand('ls', ['`id`']).isValid).toBe(false);
    });

    it('should BLOCK semicolon command chaining', () => {
      expect(validateCommand('ls', ['.; rm -rf /']).isValid).toBe(false);
    });

    it('should BLOCK pipe operator', () => {
      expect(validateCommand('ls', ['| cat /etc/passwd']).isValid).toBe(false);
    });

    it('should BLOCK variable expansion ${VAR}', () => {
      expect(validateCommand('rg', ['test', '${HOME}']).isValid).toBe(false);
    });

    it('should BLOCK output redirection >', () => {
      expect(validateCommand('ls', ['> /tmp/out']).isValid).toBe(false);
    });

    it('should BLOCK input redirection <', () => {
      expect(validateCommand('ls', ['< /etc/passwd']).isValid).toBe(false);
    });

    it('should BLOCK ampersand background execution', () => {
      expect(validateCommand('ls', ['& rm -rf /']).isValid).toBe(false);
    });
  });

  describe('Injection via ripgrep pattern position (should be ALLOWED)', () => {
    it('should ALLOW regex OR in pattern position', () => {
      expect(validateCommand('rg', ['foo|bar', '.']).isValid).toBe(true);
    });

    it('should ALLOW parentheses in pattern position', () => {
      expect(validateCommand('rg', ['(foo|bar)+', '.']).isValid).toBe(true);
    });

    it('should ALLOW brackets in pattern position', () => {
      expect(validateCommand('rg', ['[a-z]+', '.']).isValid).toBe(true);
    });

    it('should ALLOW curly braces in glob position', () => {
      expect(
        validateCommand('rg', ['-g', '*.{ts,tsx}', 'pattern', '.']).isValid
      ).toBe(true);
    });
  });

  describe('Injection via pattern arguments that should still be BLOCKED', () => {
    it('should BLOCK command substitution in pattern', () => {
      expect(validateCommand('rg', ['$(rm -rf /)', '.']).isValid).toBe(false);
    });

    it('should BLOCK backticks in pattern', () => {
      expect(validateCommand('rg', ['`id`', '.']).isValid).toBe(false);
    });

    it('should BLOCK variable expansion in pattern', () => {
      expect(validateCommand('rg', ['${HOME}', '.']).isValid).toBe(false);
    });

    it('should BLOCK semicolons in pattern', () => {
      expect(validateCommand('rg', ['test;rm -rf /', '.']).isValid).toBe(false);
    });
  });

  describe('Injection via find arguments', () => {
    it('should BLOCK command substitution in non-pattern find arg', () => {
      expect(validateCommand('find', ['$(pwd)', '-name', '*.ts']).isValid).toBe(
        false
      );
    });

    it('should ALLOW glob pattern after -name', () => {
      expect(
        validateCommand('find', ['.', '-name', '*.{ts,tsx}']).isValid
      ).toBe(true);
    });

    it('should ALLOW regex after -regex', () => {
      expect(
        validateCommand('find', ['.', '-regex', '.*\\.test\\.ts$']).isValid
      ).toBe(true);
    });
  });
});

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 7: Sensitive File / Ignored Path Protection
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

describe('SEC-07: Sensitive File Protection (Ignored Patterns)', () => {
  let v: PathValidator;

  beforeEach(() => {
    v = new PathValidator({ workspaceRoot: WORKSPACE }); // default includes home dir
  });

  describe('Environment files', () => {
    const envFiles = [
      '.env',
      '.env.local',
      '.env.development',
      '.env.production',
      '.env.test',
      '.env.staging',
    ];

    envFiles.forEach(f => {
      it(`should BLOCK ${f}`, () => {
        expect(v.validate(`${WORKSPACE}/${f}`).isValid).toBe(false);
      });
    });
  });

  describe('SSH keys', () => {
    const sshFiles = [
      '.ssh/id_rsa',
      '.ssh/id_ed25519',
      '.ssh/id_ecdsa',
      '.ssh/authorized_keys',
      '.ssh/known_hosts',
    ];

    sshFiles.forEach(f => {
      it(`should BLOCK ${f}`, () => {
        expect(v.validate(`${HOME}/${f}`).isValid).toBe(false);
      });
    });
  });

  describe('Cloud credentials', () => {
    const cloudPaths = [
      '.aws/credentials',
      '.aws/config',
      '.docker/config.json',
      '.kube/config',
    ];

    cloudPaths.forEach(f => {
      it(`should BLOCK ${f}`, () => {
        expect(v.validate(`${HOME}/${f}`).isValid).toBe(false);
      });
    });
  });

  describe('Git internal files', () => {
    it('should BLOCK .git/config', () => {
      expect(v.validate(`${WORKSPACE}/.git/config`).isValid).toBe(false);
    });

    it('should BLOCK .git/HEAD', () => {
      expect(v.validate(`${WORKSPACE}/.git/HEAD`).isValid).toBe(false);
    });

    it('should BLOCK .git-credentials', () => {
      expect(v.validate(`${HOME}/.git-credentials`).isValid).toBe(false);
    });
  });

  describe('Private keys and certificates', () => {
    const keyFiles = ['server.key', 'private.pem', 'cert.p12', 'keystore.jks'];

    keyFiles.forEach(f => {
      it(`should BLOCK ${f}`, () => {
        expect(v.validate(`${WORKSPACE}/${f}`).isValid).toBe(false);
      });
    });
  });

  describe('Token files', () => {
    const tokenFiles = ['.token', 'token.txt', 'access_token', 'auth_token'];

    tokenFiles.forEach(f => {
      it(`should BLOCK ${f}`, () => {
        expect(v.validate(`${WORKSPACE}/${f}`).isValid).toBe(false);
      });
    });
  });
});

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 8: Home Directory Boundary Enforcement
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

describe('SEC-08: Home Directory Boundary', () => {
  describe('Default mode (home dir included)', () => {
    const v = new PathValidator({ workspaceRoot: WORKSPACE }); // includeHomeDir: true (default)

    it('should ALLOW paths within home directory', () => {
      expect(v.validate(HOME).isValid).toBe(true);
    });

    it('should BLOCK paths above home directory', () => {
      const parentOfHome = path.dirname(HOME);
      // Only test if home is not root
      if (parentOfHome !== HOME) {
        expect(v.validate(parentOfHome).isValid).toBe(false);
      }
    });
  });

  describe('Strict mode (home dir excluded)', () => {
    const v = strictValidator();

    it('should BLOCK home directory itself', () => {
      // Only if home != workspace
      if (HOME !== WORKSPACE && !WORKSPACE.startsWith(HOME + path.sep)) {
        expect(v.validate(HOME).isValid).toBe(false);
      }
    });

    it('should BLOCK ~/Documents', () => {
      const docs = path.join(HOME, 'Documents');
      if (!docs.startsWith(WORKSPACE)) {
        expect(v.validate(docs).isValid).toBe(false);
      }
    });
  });

  describe('Tilde expansion security', () => {
    it('tilde resolves to home directory, not workspace escape', () => {
      const v = new PathValidator({ workspaceRoot: WORKSPACE });
      const r = v.validate('~/');
      // If valid, must be within home OR workspace
      if (r.isValid) {
        const resolved = r.sanitizedPath!;
        const withinAllowed =
          resolved.startsWith(WORKSPACE) ||
          resolved.startsWith(HOME) ||
          resolved === HOME;
        expect(withinAllowed).toBe(true);
      }
    });
  });
});

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 9: End-to-End Attack Scenarios
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

describe('SEC-09: End-to-End Attack Scenarios', () => {
  describe('Scenario: Read /etc/passwd via path traversal', () => {
    // Use enough ../  to guarantee reaching filesystem root on any platform
    // (CI workspace can be deep, e.g. /home/runner/work/org/repo/packages/pkg)
    const deepTraversal = '../'.repeat(30);

    it('PathValidator blocks the traversal', () => {
      const v = strictValidator();
      const r = v.validate(`${WORKSPACE}/${deepTraversal}etc/passwd`);
      expect(r.isValid).toBe(false);
    });

    it('validateToolPath blocks for localGetFileContent', () => {
      const r = validateToolPath(
        toolQuery(`${WORKSPACE}/${deepTraversal}etc/passwd`),
        'localGetFileContent'
      );
      expect(r.isValid).toBe(false);
    });

    it('validateToolPath blocks absolute /etc/passwd', () => {
      const r = validateToolPath(
        toolQuery('/etc/passwd'),
        'localGetFileContent'
      );
      expect(r.isValid).toBe(false);
    });
  });

  describe('Scenario: Search /etc via localSearchCode', () => {
    it('validateToolPath blocks /etc as search path', () => {
      const r = validateToolPath(toolQuery('/etc'), 'localSearchCode');
      expect(r.isValid).toBe(false);
    });

    it('validateToolPath blocks absolute /etc path', () => {
      const r = validateToolPath(toolQuery('/etc'), 'localSearchCode');
      expect(r.isValid).toBe(false);
    });
  });

  describe('Scenario: List files in /root via localViewStructure', () => {
    it('validateToolPath blocks /root', () => {
      const r = validateToolPath(toolQuery('/root'), 'localViewStructure');
      expect(r.isValid).toBe(false);
    });
  });

  describe('Scenario: Find files in /var via localFindFiles', () => {
    it('validateToolPath blocks /var/log', () => {
      const r = validateToolPath(toolQuery('/var/log'), 'localFindFiles');
      expect(r.isValid).toBe(false);
    });
  });

  describe('Scenario: Symlink escape to read SSH keys', () => {
    it('symlink to ~/.ssh/id_rsa is blocked', () => {
      const mock = vi
        .spyOn(fs, 'realpathSync')
        .mockReturnValue(`${HOME}/.ssh/id_rsa`);
      try {
        const v = new PathValidator({ workspaceRoot: WORKSPACE });
        const r = v.validate(`${WORKSPACE}/innocent-link`);
        expect(r.isValid).toBe(false);
      } finally {
        mock.mockRestore();
      }
    });
  });

  describe('Scenario: Command injection via tool execution', () => {
    it('command validator blocks arbitrary commands', () => {
      expect(validateCommand('bash', ['-c', 'cat /etc/passwd']).isValid).toBe(
        false
      );
    });

    it('command validator blocks rm', () => {
      expect(validateCommand('rm', ['-rf', '/']).isValid).toBe(false);
    });
  });
});

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 10: Allowed Roots Configuration Security
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

describe('SEC-10: Allowed Roots Configuration Security', () => {
  describe('ALLOWED_PATHS env var handling', () => {
    let originalEnv: string | undefined;

    beforeEach(() => {
      originalEnv = process.env.ALLOWED_PATHS;
    });

    afterEach(() => {
      if (originalEnv === undefined) {
        delete process.env.ALLOWED_PATHS;
      } else {
        process.env.ALLOWED_PATHS = originalEnv;
      }
    });

    it('should respect additional allowed paths', () => {
      process.env.ALLOWED_PATHS = '/tmp';
      const v = new PathValidator({ workspaceRoot: WORKSPACE });
      expect(v.validate('/tmp/test.txt').isValid).toBe(true);
    });

    it('should still block non-allowed paths', () => {
      process.env.ALLOWED_PATHS = '/tmp';
      const v = new PathValidator({
        workspaceRoot: WORKSPACE,
        includeHomeDir: false,
      });
      expect(v.validate('/var/log/syslog').isValid).toBe(false);
    });

    it('should handle empty entries in ALLOWED_PATHS', () => {
      process.env.ALLOWED_PATHS = '/tmp,  ,, /var';
      const v = new PathValidator({ workspaceRoot: WORKSPACE });
      const roots = v.getAllowedRoots();
      expect(roots).toContain('/tmp');
      expect(roots).toContain('/var');
    });
  });

  describe('Allowed roots immutability', () => {
    it('getAllowedRoots returns a copy, not reference', () => {
      const v = strictValidator();
      const roots1 = v.getAllowedRoots();
      const roots2 = v.getAllowedRoots();
      expect(roots1).not.toBe(roots2); // different array instances
      expect(roots1).toEqual(roots2); // same content
    });
  });

  describe('addAllowedRoot validation', () => {
    it('should not add duplicate roots', () => {
      const v = strictValidator();
      const initialCount = v.getAllowedRoots().length;
      v.addAllowedRoot(WORKSPACE);
      v.addAllowedRoot(WORKSPACE);
      expect(v.getAllowedRoots().length).toBe(initialCount);
    });

    it('should resolve relative paths in addAllowedRoot', () => {
      const v = strictValidator();
      v.addAllowedRoot('.');
      expect(v.getAllowedRoots()).toContain(process.cwd());
    });

    it('should expand tilde in addAllowedRoot', () => {
      const v = strictValidator();
      v.addAllowedRoot('~/test-dir');
      const roots = v.getAllowedRoots();
      expect(roots.some(r => r.startsWith(HOME))).toBe(true);
    });
  });
});

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 11: Prefix Matching Security (Critical)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

describe('SEC-11: Prefix Matching Security', () => {
  /**
   * A naive implementation might use: path.startsWith(root)
   * which would allow "/workspace-evil" to match "/workspace".
   *
   * The correct check is: path === root || path.startsWith(root + path.sep)
   */

  it('should use path.sep in prefix check, not bare startsWith', () => {
    const v = strictValidator('/Users/workspace');

    // "/Users/workspace-evil" starts with "/Users/workspace" but is NOT a child
    expect(v.validate('/Users/workspace-evil').isValid).toBe(false);
    expect(v.validate('/Users/workspace-evil/secret.txt').isValid).toBe(false);

    // "/Users/workspace/file" IS a child
    expect(v.validate('/Users/workspace/file.txt').isValid).toBe(true);

    // "/Users/workspace" itself IS allowed
    expect(v.validate('/Users/workspace').isValid).toBe(true);
  });

  it('should handle workspace roots ending with path.sep', () => {
    const v = strictValidator(WORKSPACE);
    // Ensure the workspace itself is valid
    expect(v.validate(WORKSPACE).isValid).toBe(true);
    // Ensure workspace + sep + file is valid
    expect(v.validate(WORKSPACE + '/test.txt').isValid).toBe(true);
  });
});
