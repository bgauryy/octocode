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

    it('should parse short boolean options', () => {
      const result = parseArgs(['-f']);
      expect(result.options).toEqual({ f: true });
    });

    it('should parse combined short options', () => {
      const result = parseArgs(['-fv']);
      expect(result.options).toEqual({ f: true, v: true });
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

    it('should parse -H option with value for hostname', () => {
      const result = parseArgs(['token', '-H', 'github.enterprise.com']);
      expect(result.command).toBe('token');
      expect(result.options).toEqual({ H: 'github.enterprise.com' });
    });

    it('should parse -h as help flag (boolean), not hostname', () => {
      const result = parseArgs(['-h']);
      expect(result.options).toEqual({ h: true });
    });

    it('should parse --type option with value', () => {
      const result = parseArgs(['token', '--type', 'gh']);
      expect(result.command).toBe('token');
      expect(result.options).toEqual({ type: 'gh' });
    });

    it('should parse -t option with value for type', () => {
      const result = parseArgs(['token', '-t', 'octocode']);
      expect(result.command).toBe('token');
      expect(result.options).toEqual({ t: 'octocode' });
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

    it('should parse skills install -k with value', () => {
      const result = parseArgs(['skills', 'install', '-k', 'octocode-roast']);
      expect(result.command).toBe('skills');
      expect(result.args).toEqual(['install']);
      expect(result.options).toEqual({ k: 'octocode-roast' });
    });

    it('should synthesize the tool command from top-level --tool usage', () => {
      const result = parseArgs([
        '--tool',
        'localSearchCode',
        '{"path":".","pattern":"runCLI"}',
      ]);

      expect(result.command).toBe('tool');
      expect(result.args).toEqual([
        'localSearchCode',
        '{"path":".","pattern":"runCLI"}',
      ]);
      expect(result.options).toEqual({
        tool: 'localSearchCode',
      });
    });

    it('should keep legacy --input available for migration errors', () => {
      const result = parseArgs([
        '--tool',
        'localSearchCode',
        '--input',
        '{"path":".","pattern":"runCLI"}',
      ]);

      expect(result.command).toBe('tool');
      expect(result.args).toEqual(['localSearchCode']);
      expect(result.options).toEqual({
        tool: 'localSearchCode',
        input: '{"path":".","pattern":"runCLI"}',
      });
    });

    it('should parse --tool with --queries flag', () => {
      const result = parseArgs([
        '--tool',
        'localSearchCode',
        '--queries',
        '{"path":".","pattern":"runCLI"}',
      ]);

      expect(result.command).toBe('tool');
      expect(result.args).toEqual(['localSearchCode']);
      expect(result.options).toEqual({
        tool: 'localSearchCode',
        queries: '{"path":".","pattern":"runCLI"}',
      });
    });

    it('should parse --tools-context as a top-level boolean flag', () => {
      const result = parseArgs(['--tools-context']);

      expect(result.command).toBeNull();
      expect(result.options).toEqual({ 'tools-context': true });
    });

    it('should parse --agent as a top-level boolean flag', () => {
      const result = parseArgs(['--agent']);

      expect(result.command).toBeNull();
      expect(result.options).toEqual({ agent: true });
    });

    it('should not let --agent swallow a following argument', () => {
      const result = parseArgs(['--agent', 'somecommand']);

      expect(result.options.agent).toBe(true);
      expect(result.command).toBe('somecommand');
    });

    it('should parse new agent/output boolean flags', () => {
      expect(parseArgs(['tools', 'x', '--compact']).options.compact).toBe(true);
      expect(parseArgs(['--agent', '--full']).options.full).toBe(true);
      expect(parseArgs(['tools', '--no-color']).options['no-color']).toBe(true);
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

    it('should parse single-dash long option -tool with = value', () => {
      const result = parseArgs(['-tool=myTool']);
      expect(result.command).toBe('tool');
      expect(result.args).toEqual(['myTool']);
      expect(result.options).toEqual({ tool: 'myTool' });
    });

    it('should parse single-dash long option -tool consuming next arg', () => {
      const result = parseArgs(['-tool', 'myTool']);
      expect(result.command).toBe('tool');
      expect(result.args).toEqual(['myTool']);
      expect(result.options).toEqual({ tool: 'myTool' });
    });

    it('should treat single-dash -tool as boolean when no value follows', () => {
      const result = parseArgs(['-tool']);
      expect(result.command).toBeNull();
      expect(result.options).toEqual({ tool: true });
    });

    it('should parse single-dash long option -output with = value', () => {
      const result = parseArgs(['--tool', 'localSearchCode', '-output=json']);
      expect(result.command).toBe('tool');
      expect(result.args).toEqual(['localSearchCode']);
      expect(result.options).toEqual({
        tool: 'localSearchCode',
        output: 'json',
      });
    });

    it('should parse single-dash long option -queries with = value', () => {
      const result = parseArgs([
        '-tool=localSearchCode',
        '-queries={"path":".","pattern":"x"}',
      ]);
      expect(result.command).toBe('tool');
      expect(result.args).toEqual(['localSearchCode']);
      expect(result.options).toEqual({
        tool: 'localSearchCode',
        queries: '{"path":".","pattern":"x"}',
      });
    });

    it('should parse single-dash long option -queries consuming next arg', () => {
      const result = parseArgs([
        '-tool',
        'localSearchCode',
        '-queries',
        '{"path":".","pattern":"next"}',
      ]);
      expect(result.command).toBe('tool');
      expect(result.args).toEqual(['localSearchCode']);
      expect(result.options).toEqual({
        tool: 'localSearchCode',
        queries: '{"path":".","pattern":"next"}',
      });
    });

    it('should consume values for unknown long flags after the tool command', () => {
      const result = parseArgs(['tool', '--extra', 'payload']);
      expect(result.command).toBe('tool');
      expect(result.args).toEqual([]);
      expect(result.options).toEqual({ extra: 'payload' });
    });
  });

  describe('hasHelpFlag', () => {
    it('should detect --help', () => {
      const args = parseArgs(['--help']);
      expect(hasHelpFlag(args)).toBe(true);
    });

    it('should detect -h', () => {
      const args = parseArgs(['-h']);
      expect(hasHelpFlag(args)).toBe(true);
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

    it('should detect -v', () => {
      const args = parseArgs(['-v']);
      expect(hasVersionFlag(args)).toBe(true);
    });

    it('should return false when no version flag', () => {
      const args = parseArgs(['install']);
      expect(hasVersionFlag(args)).toBe(false);
    });
  });
});
