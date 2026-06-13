import { describe, it, expect } from 'vitest';
import {
  parseArgs,
  hasHelpFlag,
  hasVersionFlag,
} from '../../src/cli/parser.js';

describe('CLI Parser', () => {
  describe('parseArgs', () => {
    it('should parse command', () => {
      const result = parseArgs(['install']);
      expect(result.command).toBe('install');
      expect(result.args).toEqual([]);
      expect(result.options).toEqual({});
    });

    it('should parse command with positional args', () => {
      const result = parseArgs(['install', 'arg1', 'arg2']);
      expect(result.command).toBe('install');
      expect(result.args).toEqual(['arg1', 'arg2']);
    });

    it('should parse long options with values using =', () => {
      const result = parseArgs(['--ide=cursor']);
      expect(result.options).toEqual({ ide: 'cursor' });
    });

    it('should parse long options with values as next arg', () => {
      const result = parseArgs(['--ide', 'cursor']);
      expect(result.options).toEqual({ ide: 'cursor' });
    });

    it('should parse boolean long options', () => {
      const result = parseArgs(['--force']);
      expect(result.options).toEqual({ force: true });
    });

    it('should parse command with options', () => {
      const result = parseArgs(['install', '--ide', 'cursor', '--force']);
      expect(result.command).toBe('install');
      expect(result.options).toEqual({ ide: 'cursor', force: true });
    });

    it('should handle empty argv', () => {
      const result = parseArgs([]);
      expect(result.command).toBeNull();
      expect(result.args).toEqual([]);
      expect(result.options).toEqual({});
    });

    it('should parse --method option', () => {
      const result = parseArgs(['install', '--method', 'npx']);
      expect(result.command).toBe('install');
      expect(result.options).toEqual({ method: 'npx' });
    });

    it('should handle options before command', () => {
      const result = parseArgs(['--help', 'install']);
      expect(result.command).toBe('install');
      expect(result.options).toEqual({ help: true });
    });

    it('should parse --hostname option', () => {
      const result = parseArgs([
        'status',
        '--hostname',
        'github.enterprise.com',
      ]);
      expect(result.command).toBe('status');
      expect(result.options).toEqual({ hostname: 'github.enterprise.com' });
    });

    it('should keep single-dash tokens positional', () => {
      const result = parseArgs(['token', '-H', 'github.enterprise.com']);
      expect(result.command).toBe('token');
      expect(result.options).toEqual({});
      expect(result.args).toEqual(['-H', 'github.enterprise.com']);
    });

    it('should parse --type option with value', () => {
      const result = parseArgs(['token', '--type', 'gh']);
      expect(result.command).toBe('token');
      expect(result.options).toEqual({ type: 'gh' });
    });

    it('should parse --git-protocol option', () => {
      const result = parseArgs(['login', '--git-protocol', 'ssh']);
      expect(result.command).toBe('login');
      expect(result.options).toEqual({ 'git-protocol': 'ssh' });
    });

    it('should parse skills install --skill with value', () => {
      const result = parseArgs([
        'skills',
        'install',
        '--skill',
        'octocode-plan',
      ]);
      expect(result.command).toBe('skills');
      expect(result.args).toEqual(['install']);
      expect(result.options).toEqual({ skill: 'octocode-plan' });
    });

    it('should keep single-dash skill tokens positional', () => {
      const result = parseArgs(['skills', 'install', '-k', 'octocode-roast']);
      expect(result.command).toBe('skills');
      expect(result.args).toEqual(['install', '-k', 'octocode-roast']);
      expect(result.options).toEqual({});
    });

    it('should parse canonical tools command with --queries', () => {
      const result = parseArgs([
        'tools',
        'localSearchCode',
        '--queries',
        '{"path":".","keywords":"runCLI"}',
      ]);

      expect(result.command).toBe('tools');
      expect(result.args).toEqual(['localSearchCode']);
      expect(result.options).toEqual({
        queries: '{"path":".","keywords":"runCLI"}',
      });
    });

    it('should parse context and scheme flags', () => {
      expect(parseArgs(['context', '--full']).options.full).toBe(true);
      expect(parseArgs(['tools', '--no-color']).options['no-color']).toBe(true);
      expect(
        parseArgs(['tools', 'localSearchCode', '--scheme']).options.scheme
      ).toBe(true);
      expect(parseArgs(['token', '--reveal']).options.reveal).toBe(true);
    });

    it('should parse --format as a value option', () => {
      expect(parseArgs(['tools', 'x', '--format', 'tool']).options.format).toBe(
        'tool'
      );
      expect(parseArgs(['tools', 'x', '--format=tool']).options.format).toBe(
        'tool'
      );
    });

    it('should parse lsp command value options', () => {
      const result = parseArgs([
        'lsp',
        'src/index.ts',
        '--type',
        'references',
        '--symbol',
        'runCLI',
        '--line',
        '42',
        '--workspace-root',
        '.',
      ]);

      expect(result.command).toBe('lsp');
      expect(result.args).toEqual(['src/index.ts']);
      expect(result.options).toEqual({
        type: 'references',
        symbol: 'runCLI',
        line: '42',
        'workspace-root': '.',
      });
    });

    it('should parse symbols command value options', () => {
      const result = parseArgs([
        'symbols',
        'src',
        '--ext',
        'ts,tsx',
        '--kind',
        'function',
        '--limit',
        '10',
      ]);

      expect(result.command).toBe('symbols');
      expect(result.args).toEqual(['src']);
      expect(result.options).toEqual({
        ext: 'ts,tsx',
        kind: 'function',
        limit: '10',
      });
    });

    it('should parse repo command value options', () => {
      const result = parseArgs([
        'repo',
        'agent',
        'tools',
        '--topic',
        'mcp,agents',
        '--language',
        'TypeScript',
        '--owner',
        'openai',
        '--stars',
        '>1000',
        '--forks',
        '>100',
        '--good-first-issues',
        '>5',
        '--license',
        'mit',
        '--created',
        '>=2024-01-01',
        '--updated',
        '>2025-01-01',
        '--size',
        '<50000',
        '--match',
        'name,description',
        '--sort',
        'stars',
        '--visibility',
        'public',
        '--archived',
        'false',
        '--verbose',
        '--limit',
        '10',
      ]);

      expect(result.command).toBe('repo');
      expect(result.args).toEqual(['agent', 'tools']);
      expect(result.options).toEqual({
        topic: 'mcp,agents',
        language: 'TypeScript',
        owner: 'openai',
        stars: '>1000',
        forks: '>100',
        'good-first-issues': '>5',
        license: 'mit',
        created: '>=2024-01-01',
        updated: '>2025-01-01',
        size: '<50000',
        match: 'name,description',
        sort: 'stars',
        visibility: 'public',
        archived: 'false',
        verbose: true,
        limit: '10',
      });
    });

    it('should parse files command value and boolean options', () => {
      const result = parseArgs([
        'files',
        'auth',
        '.',
        '--source',
        'local',
        '--search',
        'both',
        '--ext',
        'ts,tsx',
        '--path',
        'src',
        '--name',
        '*auth*',
        '--regex',
        'auth.*config',
        '--entry',
        'f',
        '--min-depth',
        '1',
        '--max-depth',
        '4',
        '--modified-within',
        '7d',
        '--include',
        '*.ts',
        '--exclude-dir',
        'node_modules,dist',
        '--context-lines',
        '3',
        '--max-matches-per-file',
        '5',
        '--match-page',
        '2',
        '--details',
        '--fixed-string',
        '--limit',
        '20',
      ]);

      expect(result.command).toBe('files');
      expect(result.args).toEqual(['auth', '.']);
      expect(result.options).toEqual({
        source: 'local',
        search: 'both',
        ext: 'ts,tsx',
        path: 'src',
        name: '*auth*',
        regex: 'auth.*config',
        entry: 'f',
        'min-depth': '1',
        'max-depth': '4',
        'modified-within': '7d',
        include: '*.ts',
        'exclude-dir': 'node_modules,dist',
        'context-lines': '3',
        'max-matches-per-file': '5',
        'match-page': '2',
        details: true,
        'fixed-string': true,
        limit: '20',
      });
    });

    it('keeps token --source boolean while files --source consumes a value', () => {
      expect(parseArgs(['token', '--source']).options.source).toBe(true);
      expect(
        parseArgs(['files', 'x', '.', '--source', 'github']).options.source
      ).toBe('github');
    });

    it('should parse unsupported top-level long options without rewriting them', () => {
      expect(parseArgs(['--not-real']).options['not-real']).toBe(true);
      expect(parseArgs(['--unknown=value']).options.unknown).toBe('value');
    });

    it('should keep unsupported top-level option values positional when space-separated', () => {
      const result = parseArgs(['--not-real', 'next-command']);
      expect(result.command).toBe('next-command');
      expect(result.options).toEqual({ 'not-real': true });
    });

    it('should consume values for unknown long flags after the tools command', () => {
      const result = parseArgs(['tools', '--extra', 'payload']);
      expect(result.command).toBe('tools');
      expect(result.args).toEqual([]);
      expect(result.options).toEqual({ extra: 'payload' });
    });
  });

  describe('hasHelpFlag', () => {
    it('should detect --help', () => {
      const args = parseArgs(['--help']);
      expect(hasHelpFlag(args)).toBe(true);
    });

    it('should ignore single-dash help spelling', () => {
      const args = parseArgs(['-h']);
      expect(hasHelpFlag(args)).toBe(false);
    });

    it('should return false when no help flag', () => {
      const args = parseArgs(['install']);
      expect(hasHelpFlag(args)).toBe(false);
    });
  });

  describe('hasVersionFlag', () => {
    it('should detect --version', () => {
      const args = parseArgs(['--version']);
      expect(hasVersionFlag(args)).toBe(true);
    });

    it('should ignore single-dash version spelling', () => {
      const args = parseArgs(['-v']);
      expect(hasVersionFlag(args)).toBe(false);
    });

    it('should return false when no version flag', () => {
      const args = parseArgs(['install']);
      expect(hasVersionFlag(args)).toBe(false);
    });
  });
});
