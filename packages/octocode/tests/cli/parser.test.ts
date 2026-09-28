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

    it('should parse skill value options as next arg', () => {
      const result = parseArgs(['--platform', 'pi']);
      expect(result.options).toEqual({ platform: 'pi' });
    });

    it('should parse boolean long options', () => {
      const result = parseArgs(['--force']);
      expect(result.options).toEqual({ force: true });
    });

    it('should parse command with options', () => {
      const result = parseArgs(['install', '--ide=cursor', '--force']);
      expect(result.command).toBe('install');
      expect(result.options).toEqual({ ide: 'cursor', force: true });
    });

    it('parses skill installation scope options', () => {
      expect(
        parseArgs(['skill', 'install', '--platform', 'codex', '--global'])
          .options
      ).toEqual({ platform: 'codex', global: true });
      expect(
        parseArgs([
          'skill',
          'install',
          '--platform',
          'codex',
          '--project-dir',
          '/tmp/project',
        ]).options
      ).toEqual({ platform: 'codex', 'project-dir': '/tmp/project' });
    });

    it('should handle empty argv', () => {
      const result = parseArgs([]);
      expect(result.command).toBeNull();
      expect(result.args).toEqual([]);
      expect(result.options).toEqual({});
    });

    it('should handle options before command', () => {
      const result = parseArgs(['--help', 'install']);
      expect(result.command).toBe('install');
      expect(result.options).toEqual({ help: true });
    });

    it('should consume values only for skill value flags', () => {
      const result = parseArgs([
        'skill',
        'install',
        'octocode-research',
        '--platform',
        'pi',
        '--project-dir',
        '/tmp/proj',
        '--global',
      ]);
      expect(result.command).toBe('skill');
      expect(result.args).toEqual(['install', 'octocode-research']);
      expect(result.options).toEqual({
        platform: 'pi',
        'project-dir': '/tmp/proj',
        global: true,
      });
    });

    it('should keep values of native-owned flags positional (raw argv is forwarded)', () => {
      const result = parseArgs(['install', '--ide', 'cursor']);
      expect(result.command).toBe('install');
      expect(result.args).toEqual(['cursor']);
      expect(result.options).toEqual({ ide: true });
    });

    it('should keep single-dash tokens positional', () => {
      const result = parseArgs(['install', '-i', 'cursor']);
      expect(result.command).toBe('install');
      expect(result.args).toEqual(['-i', 'cursor']);
      expect(result.options).toEqual({});
    });

    it('should treat a tool query as positional under a tool command', () => {
      const result = parseArgs([
        'localSearch',
        '{"path":".","searchText":"runCLI"}',
        '--pretty',
      ]);
      expect(result.command).toBe('localSearch');
      expect(result.args).toEqual(['{"path":".","searchText":"runCLI"}']);
      expect(result.options).toEqual({ pretty: true });
    });

    it('should parse boolean flags without swallowing following tokens', () => {
      expect(parseArgs(['scheme', '--compact']).options.compact).toBe(true);
      expect(parseArgs(['scheme', '--no-color']).options['no-color']).toBe(
        true
      );
      expect(parseArgs(['auth', '--json']).options.json).toBe(true);
    });

    it('should parse --key=value regardless of the value list', () => {
      expect(parseArgs(['scheme', 'x', '--view=query']).options.view).toBe(
        'query'
      );
    });

    it('treats retired command vocabulary as plain booleans', () => {
      // Native commands re-parse raw argv themselves, so the parser carries
      // no per-command vocabulary beyond the skill value flags.
      const result = parseArgs(['ghSearchRepo', '--stars', '5']);
      expect(result.options).toEqual({ stars: true });
      expect(result.args).toEqual(['5']);
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

    it('should keep unknown long-flag values positional under any command', () => {
      const result = parseArgs(['localSearch', '--extra', 'payload']);
      expect(result.command).toBe('localSearch');
      expect(result.args).toEqual(['payload']);
      expect(result.options).toEqual({ extra: true });
    });

    it('should skip a standalone "--" separator (npm/yarn style) and keep parsing', () => {
      const result = parseArgs(['--', 'query', '@x/y', '--json']);
      expect(result.command).toBe('query');
      expect(result.args).toEqual(['@x/y']);
      expect(result.options).toEqual({ json: true });
      // never produces an empty-string option key
      expect(Object.keys(result.options)).not.toContain('');
    });

    it('should skip "--" anywhere in the argv, not just at the front', () => {
      const result = parseArgs(['query', 'zod', '--', '--mode', 'lean']);
      expect(result.command).toBe('query');
      expect(result.args).toEqual(['zod']);
      expect(result.options).toEqual({ mode: 'lean' });
    });
  });

  describe('hasHelpFlag', () => {
    it('should detect --help', () => {
      const args = parseArgs(['--help']);
      expect(hasHelpFlag(args)).toBe(true);
    });

    it('should recognize single-dash help without treating it as a command', () => {
      const args = parseArgs(['-h']);
      expect(hasHelpFlag(args)).toBe(true);
      expect(args.command).toBeNull();
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
