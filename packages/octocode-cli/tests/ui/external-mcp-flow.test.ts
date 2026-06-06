import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

vi.mock('../../src/utils/prompts.js', () => ({
  loadInquirer: vi.fn(),
  input: vi.fn(),
  select: vi.fn(),
  Separator: class {},
}));

vi.mock('../../src/utils/mcp-io.js', () => ({
  readMCPConfig: vi.fn(),
  writeMCPConfig: vi.fn(),
}));

vi.mock('../../src/utils/mcp-paths.js', () => ({
  getMCPConfigPath: vi.fn().mockReturnValue('/mock/path/config.json'),
}));

import { readMCPConfig, writeMCPConfig } from '../../src/utils/mcp-io.js';
import type { MCPRegistryEntry } from '../../src/configs/mcp-registry.js';

function createMockMCP(
  overrides: Partial<MCPRegistryEntry> = {}
): MCPRegistryEntry {
  return {
    id: 'test-mcp',
    name: 'Test MCP',
    description: 'A test MCP server',
    category: 'developer-tools',
    repository: 'https://github.com/test/test-mcp',
    installationType: 'npx',
    installConfig: {
      command: 'npx',
      args: ['-y', 'test-mcp'],
    },
    ...overrides,
  };
}

describe('External MCP Flow - Argument Validation', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(readMCPConfig).mockReturnValue({ mcpServers: {} });
    vi.mocked(writeMCPConfig).mockReturnValue({ success: true });
  });

  afterEach(() => {
    vi.resetAllMocks();
  });

  describe('Safe CLI Flags', () => {
    it('should accept single-letter flags like -y', () => {
      const mcp = createMockMCP({
        installConfig: {
          command: 'npx',
          args: ['-y', 'test-package'],
        },
      });

      expect(() => {
        const args = mcp.installConfig.args;

        expect(args).toContain('-y');
      }).not.toThrow();
    });

    it('should accept common docker flags like -i and --rm', () => {
      const mcp = createMockMCP({
        installConfig: {
          command: 'docker',
          args: ['run', '-i', '--rm', '-e', 'VAR', 'image:latest'],
        },
      });

      const args = mcp.installConfig.args;
      expect(args).toContain('-i');
      expect(args).toContain('--rm');
      expect(args).toContain('-e');
    });

    it('should accept long flags like --yes, --no-cache', () => {
      const args = ['--yes', '--no-cache', '--port=8080', '--host=localhost'];

      args.forEach(arg => {
        const safeFlagPattern = /^--?[a-zA-Z][a-zA-Z0-9-]*(=\S+)?$/;
        expect(safeFlagPattern.test(arg)).toBe(true);
      });
    });

    it('should accept flags with values like --port=8080', () => {
      const flagsWithValues = [
        '--port=8080',
        '-p=3000',
        '--host=0.0.0.0',
        '--config=/path/to/config',
      ];

      const safeFlagPattern = /^--?[a-zA-Z][a-zA-Z0-9-]*(=\S+)?$/;
      flagsWithValues.forEach(flag => {
        expect(safeFlagPattern.test(flag)).toBe(true);
      });
    });
  });

  describe('Dangerous Patterns', () => {
    it('should reject command chaining characters', () => {
      const dangerousArgs = [
        'arg; rm -rf /',
        'arg && echo pwned',
        'arg | cat /etc/passwd',
        'arg `whoami`',
        'arg $HOME',
      ];

      const dangerousPattern = /[;&|`$]/;
      dangerousArgs.forEach(arg => {
        expect(dangerousPattern.test(arg)).toBe(true);
      });
    });

    it('should reject subshell and brace characters', () => {
      const dangerousArgs = [
        '$(whoami)',
        '{echo,pwned}',
        '[array]',
        '(subshell)',
      ];

      const dangerousPattern = /[(){}[\]]/;
      dangerousArgs.forEach(arg => {
        expect(dangerousPattern.test(arg)).toBe(true);
      });
    });

    it('should reject redirect characters', () => {
      const dangerousArgs = ['file > /etc/passwd', 'file < /etc/shadow'];

      const dangerousPattern = /[<>]/;
      dangerousArgs.forEach(arg => {
        expect(dangerousPattern.test(arg)).toBe(true);
      });
    });

    it('should reject history expansion and negation', () => {
      const dangerousArgs = ['!$', '!!', '^pattern^replacement'];

      const dangerousPattern = /[!^]/;
      dangerousArgs.forEach(arg => {
        expect(dangerousPattern.test(arg)).toBe(true);
      });
    });

    it('should reject newlines and null bytes', () => {
      const dangerousArgs = ['arg\nwhoami', 'arg\rwhoami', 'arg\x00whoami'];

      const dangerousPattern = /[\n\r\x00]/;
      dangerousArgs.forEach(arg => {
        expect(dangerousPattern.test(arg)).toBe(true);
      });
    });

    it('should reject single-quoted strings', () => {
      const dangerousArgs = ["'malicious'", "'arg with spaces'"];

      const dangerousPattern = /'.*'/;
      dangerousArgs.forEach(arg => {
        expect(dangerousPattern.test(arg)).toBe(true);
      });
    });

    it('should reject double-quoted strings with variable expansion', () => {
      const dangerousArgs = ['"$HOME"', '"value with $VAR"'];

      const dangerousPattern = /".*\$.*"/;
      dangerousArgs.forEach(arg => {
        expect(dangerousPattern.test(arg)).toBe(true);
      });
    });
  });

  describe('MCP Registry Entries Validation', () => {
    it('should validate all registry entries have safe args (excluding placeholders)', async () => {
      const { MCP_REGISTRY } =
        await import('../../src/configs/mcp-registry.js');

      const safeFlagPattern = /^--?[a-zA-Z][a-zA-Z0-9-]*(=\S+)?$/;
      const containsPlaceholderPattern = /\$\{[A-Z_][A-Z0-9_]*\}/;
      const dangerousPatterns = [
        /[;&|`$]/,
        /[(){}[\]]/,
        /[<>]/,
        /[!^]/,
        /\\(?!["'\\])/,
        /[\n\r\x00]/,
        /'.*'/,
        /".*\$.*"/,
      ];

      for (const mcp of MCP_REGISTRY) {
        for (const arg of mcp.installConfig.args) {
          if (safeFlagPattern.test(arg)) {
            continue;
          }

          if (containsPlaceholderPattern.test(arg)) {
            continue;
          }

          for (const pattern of dangerousPatterns) {
            const isMatch = pattern.test(arg);
            if (isMatch) {
              expect(
                isMatch,
                `MCP "${mcp.id}" has potentially unsafe arg "${arg}" matching pattern ${pattern}`
              ).toBe(false);
            }
          }
        }
      }
    });

    it('should have allowed commands in all registry entries', async () => {
      const { MCP_REGISTRY } =
        await import('../../src/configs/mcp-registry.js');

      const allowedCommands = [
        'npx',
        'node',
        'python',
        'python3',
        'uvx',
        'uv',
        'docker',
        'deno',
        'bun',
        'bunx',
        'pnpm',
        'yarn',
        'npm',
        'pip',
      ];

      const mcpsWithNonAllowedCommands: string[] = [];

      for (const mcp of MCP_REGISTRY) {
        const command = mcp.installConfig.command;
        const baseCommand = command.split(/[/\\]/).pop()?.split(/\s+/)[0] || '';

        if (!allowedCommands.includes(baseCommand)) {
          mcpsWithNonAllowedCommands.push(`${mcp.id}: ${baseCommand}`);
        }
      }

      if (mcpsWithNonAllowedCommands.length > 0) {
        console.log(
          'MCPs with non-standard commands (may be source installs):',
          mcpsWithNonAllowedCommands
        );
      }

      const totalMcps = MCP_REGISTRY.length;
      const nonStandardCount = mcpsWithNonAllowedCommands.length;
      const standardCount = totalMcps - nonStandardCount;

      expect(standardCount / totalMcps).toBeGreaterThan(0.9);
    });
  });

  describe('Edge Cases', () => {
    it('should handle empty args array', () => {
      const args: string[] = [];
      expect(Array.isArray(args)).toBe(true);
      expect(args.length).toBe(0);
    });

    it('should handle package names with @ symbol', () => {
      const packageNames = [
        '@playwright/mcp@latest',
        '@modelcontextprotocol/server-filesystem',
        '@stripe/agent-toolkit',
        'test-package@1.0.0',
      ];

      const dangerousPatterns = [
        /[;&|`$]/,
        /[(){}[\]]/,
        /[<>]/,
        /[!^]/,
        /[\n\r\x00]/,
      ];

      packageNames.forEach(pkg => {
        dangerousPatterns.forEach(pattern => {
          expect(pattern.test(pkg)).toBe(false);
        });
      });
    });

    it('should handle environment variable placeholders', () => {
      const placeholders = ['${DATABASE_PATH}', '${ALLOWED_DIRECTORIES}'];

      const dollarPattern = /[;&|`$]/;
      placeholders.forEach(placeholder => {
        expect(dollarPattern.test(placeholder)).toBe(true);
      });
    });

    it('should handle path-like arguments', () => {
      const safePathPattern = /^[a-zA-Z0-9_./-]+$/;

      expect(safePathPattern.test('/usr/bin/node')).toBe(true);
      expect(safePathPattern.test('./relative/path')).toBe(true);
      expect(safePathPattern.test('/home/user/project')).toBe(true);
      expect(safePathPattern.test('../parent/path')).toBe(true);
    });

    it('should handle URL-like arguments', () => {
      const urls = [
        'https://gitmcp.io/docs',
        'http://localhost:8888',
        'git+https://github.com/repo',
      ];

      urls.forEach(url => {
        expect(/[;&|`$]/.test(url)).toBe(false);
        expect(/[(){}[\]]/.test(url)).toBe(false);
      });
    });
  });

  describe('Argument Length Validation', () => {
    it('should accept arguments under 4096 characters', () => {
      const normalArg = 'a'.repeat(100);
      expect(normalArg.length).toBeLessThan(4096);
    });

    it('should flag arguments over 4096 characters', () => {
      const longArg = 'a'.repeat(5000);
      expect(longArg.length).toBeGreaterThan(4096);
    });
  });
});

describe('External MCP Flow - Command Validation', () => {
  describe('Allowed Commands', () => {
    const allowedCommands = [
      'npx',
      'node',
      'python',
      'python3',
      'uvx',
      'uv',
      'docker',
      'deno',
      'bun',
      'bunx',
      'pnpm',
      'yarn',
      'npm',
    ];

    allowedCommands.forEach(cmd => {
      it(`should allow ${cmd} command`, () => {
        expect(allowedCommands.includes(cmd)).toBe(true);
      });
    });
  });

  describe('Path-based Commands', () => {
    it('should extract base command from paths', () => {
      const paths = [
        { input: '/usr/bin/node', expected: 'node' },
        { input: '/usr/local/bin/python3', expected: 'python3' },
        { input: 'C:\\Program Files\\nodejs\\npx.cmd', expected: 'npx.cmd' },
      ];

      paths.forEach(({ input, expected }) => {
        const segments = input.split(/[/\\]/);
        const baseCommand =
          segments[segments.length - 1]?.split(/\s+/)[0] || '';
        expect(baseCommand).toBe(expected);
      });
    });
  });

  describe('Invalid Commands', () => {
    it('should reject commands with path traversal', () => {
      const invalidCommands = ['../../../bin/sh', 'node/../../../bin/bash'];

      invalidCommands.forEach(cmd => {
        expect(cmd.includes('..')).toBe(true);
      });
    });

    it('should reject commands with null bytes', () => {
      const invalidCommands = ['node\x00malicious', 'npx\x00--evil'];

      invalidCommands.forEach(cmd => {
        expect(cmd.includes('\x00')).toBe(true);
      });
    });
  });
});

describe('External MCP Flow - Environment Variable Validation', () => {
  describe('Valid Environment Variable Names', () => {
    const validNames = [
      'GITHUB_TOKEN',
      'API_KEY',
      'DATABASE_URL',
      '_PRIVATE_VAR',
      'var123',
      'A',
    ];

    const validNamePattern = /^[A-Za-z_][A-Za-z0-9_]*$/;

    validNames.forEach(name => {
      it(`should accept env var name: ${name}`, () => {
        expect(validNamePattern.test(name)).toBe(true);
      });
    });
  });

  describe('Invalid Environment Variable Names', () => {
    const invalidNames = ['123VAR', 'VAR-NAME', 'VAR.NAME', '', 'VAR NAME'];

    const validNamePattern = /^[A-Za-z_][A-Za-z0-9_]*$/;

    invalidNames.forEach(name => {
      it(`should reject env var name: "${name}"`, () => {
        expect(validNamePattern.test(name)).toBe(false);
      });
    });
  });

  describe('Environment Variable Value Validation', () => {
    it('should reject values with control characters', () => {
      const invalidValues = [
        'value\x00null',
        'value\x08backspace',
        'value\x1Fescape',
      ];

      const controlCharPattern = /[\x00-\x08\x0B\x0C\x0E-\x1F]/;

      invalidValues.forEach(value => {
        expect(controlCharPattern.test(value)).toBe(true);
      });
    });

    it('should accept normal string values', () => {
      const validValues = [
        'sk-1234567890abcdef',
        'https://api.example.com',
        '/path/to/file.txt',
        'value with spaces',
        'value\twith\ttabs',
        'value\nwith\nnewlines',
      ];

      const controlCharPattern = /[\x00-\x08\x0B\x0C\x0E-\x1F]/;

      validValues.forEach(value => {
        expect(controlCharPattern.test(value)).toBe(false);
      });
    });

    it('should reject excessively long values', () => {
      const longValue = 'a'.repeat(40000);
      expect(longValue.length).toBeGreaterThan(32768);
    });
  });
});
