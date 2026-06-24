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

    it('should keep single-dash command args positional', () => {
      const result = parseArgs(['status', '-H', 'github.enterprise.com']);
      expect(result.command).toBe('status');
      expect(result.options).toEqual({});
      expect(result.args).toEqual(['-H', 'github.enterprise.com']);
    });

    it('should parse --hostname option with value', () => {
      const result = parseArgs(['status', '--hostname', 'github.com']);
      expect(result.command).toBe('status');
      expect(result.options).toEqual({ hostname: 'github.com' });
    });

    it('should parse --git-protocol option', () => {
      const result = parseArgs(['login', '--git-protocol', 'ssh']);
      expect(result.command).toBe('login');
      expect(result.options).toEqual({ 'git-protocol': 'ssh' });
    });

    it('should parse install --ide with value', () => {
      const result = parseArgs(['install', '--ide', 'cursor']);
      expect(result.command).toBe('install');
      expect(result.args).toEqual([]);
      expect(result.options).toEqual({ ide: 'cursor' });
    });

    it('should keep single-dash install tokens positional', () => {
      const result = parseArgs(['install', '-i', 'cursor']);
      expect(result.command).toBe('install');
      expect(result.args).toEqual(['-i', 'cursor']);
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
      expect(parseArgs(['status', '--json']).options.json).toBe(true);
    });

    it('should parse --format as a value option', () => {
      expect(parseArgs(['tools', 'x', '--format', 'tool']).options.format).toBe(
        'tool'
      );
      expect(parseArgs(['tools', 'x', '--format=tool']).options.format).toBe(
        'tool'
      );
    });

    it('should parse search semantic value options', () => {
      const result = parseArgs([
        'search',
        'src/index.ts',
        '--op',
        'references',
        '--symbol',
        'runCLI',
        '--line',
        '42',
        '--workspace-root',
        '.',
      ]);

      expect(result.command).toBe('search');
      expect(result.args).toEqual(['src/index.ts']);
      expect(result.options).toEqual({
        op: 'references',
        symbol: 'runCLI',
        line: '42',
        'workspace-root': '.',
      });
    });

    it('should parse search --items-per-page as a value option', () => {
      const result = parseArgs([
        'search',
        './src',
        '--target',
        'research',
        '--items-per-page',
        '1',
      ]);

      expect(result.command).toBe('search');
      expect(result.args).toEqual(['./src']);
      expect(result.options).toMatchObject({
        target: 'research',
        'items-per-page': '1',
      });
    });

    it('should parse search symbol-outline value options', () => {
      const result = parseArgs([
        'search',
        'src',
        '--symbols',
        '--ext',
        'ts,tsx',
        '--kind',
        'function',
        '--limit',
        '10',
      ]);

      expect(result.command).toBe('search');
      expect(result.args).toEqual(['src']);
      expect(result.options).toEqual({
        symbols: true,
        ext: 'ts,tsx',
        kind: 'function',
        limit: '10',
      });
    });

    it('should parse search repository value options', () => {
      const result = parseArgs([
        'search',
        'agent',
        'tools',
        '--target',
        'repositories',
        '--topic',
        'mcp,agents',
        '--lang',
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

      expect(result.command).toBe('search');
      expect(result.args).toEqual(['agent', 'tools']);
      expect(result.options).toEqual({
        target: 'repositories',
        topic: 'mcp,agents',
        lang: 'TypeScript',
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

    it('should parse search file-discovery value and boolean options', () => {
      const result = parseArgs([
        'search',
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
        'file',
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
        '--max-matches',
        '5',
        '--match-page',
        '2',
        '--details',
        '--fixed',
        '--limit',
        '20',
      ]);

      expect(result.command).toBe('search');
      expect(result.args).toEqual(['auth', '.']);
      expect(result.options).toEqual({
        source: 'local',
        search: 'both',
        ext: 'ts,tsx',
        path: 'src',
        name: '*auth*',
        regex: 'auth.*config',
        entry: 'file',
        'min-depth': '1',
        'max-depth': '4',
        'modified-within': '7d',
        include: '*.ts',
        'exclude-dir': 'node_modules,dist',
        'context-lines': '3',
        'max-matches': '5',
        'match-page': '2',
        details: true,
        fixed: true,
        limit: '20',
      });
    });

    it('keeps other --source boolean while search --source consumes a value', () => {
      expect(parseArgs(['status', '--source']).options.source).toBe(true);
      expect(
        parseArgs(['search', 'x', '.', '--source', 'github']).options.source
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

    it('should skip a standalone "--" separator (npm/yarn style) and keep parsing', () => {
      const result = parseArgs(['--', 'search', '@x/y', '--json']);
      expect(result.command).toBe('search');
      expect(result.args).toEqual(['@x/y']);
      expect(result.options).toEqual({ json: true });
      // never produces an empty-string option key
      expect(Object.keys(result.options)).not.toContain('');
    });

    it('should skip "--" anywhere in the argv, not just at the front', () => {
      const result = parseArgs(['search', 'zod', '--', '--mode', 'lean']);
      expect(result.command).toBe('search');
      expect(result.args).toEqual(['zod']);
      expect(result.options).toEqual({ mode: 'lean' });
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
