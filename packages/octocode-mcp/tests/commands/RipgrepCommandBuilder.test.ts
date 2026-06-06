import { describe, it, expect } from 'vitest';
import { RipgrepCommandBuilder } from '../../src/commands/RipgrepCommandBuilder.js';
import { RESOURCE_LIMITS } from '../../src/utils/core/constants.js';

function buildArgs(query: Record<string, unknown>): string[] {
  return new RipgrepCommandBuilder()
    .fromQuery({ pattern: 'foo', path: '/repo', ...query } as never)
    .build().args;
}

describe('RipgrepCommandBuilder', () => {
  describe('build()', () => {
    it('resolves a ripgrep binary as the command', () => {
      const { command } = new RipgrepCommandBuilder().build();
      expect(command).toBeTruthy();
      expect(command).not.toBe('rg');
    });
  });

  describe('fromQuery — argument-vector characterization', () => {
    it('builds a default query (smart-case, json, default match cap)', () => {
      const args = buildArgs({});
      expect(args).toEqual([
        '-S',
        '-n',
        '--column',
        '-m',
        String(RESOURCE_LIMITS.DEFAULT_MATCHES_PER_PAGE),
        '--json',
        '-j',
        '4',
        '--sort',
        'path',
        '--color',
        'never',
        '--',
        'foo',
        '/repo',
      ]);
    });

    it('terminates option parsing with -- before pattern and path', () => {
      const args = buildArgs({ pattern: '-rf', path: '/repo' });
      const dashDash = args.indexOf('--');
      expect(dashDash).toBeGreaterThanOrEqual(0);
      expect(args.slice(dashDash)).toEqual(['--', '-rf', '/repo']);
    });

    it('maps case + pattern-type flags', () => {
      expect(buildArgs({ fixedString: true, caseSensitive: true })).toContain(
        '-F'
      );
      expect(buildArgs({ fixedString: true, caseSensitive: true })).toContain(
        '-s'
      );
      expect(buildArgs({ perlRegex: true, caseInsensitive: true })).toContain(
        '-P'
      );
      expect(buildArgs({ perlRegex: true, caseInsensitive: true })).toContain(
        '-i'
      );
    });

    it('prefers -C contextLines over -A/-B', () => {
      const args = buildArgs({
        contextLines: 3,
        beforeContext: 9,
        afterContext: 9,
      });
      expect(args).toContain('-C');
      expect(args[args.indexOf('-C') + 1]).toBe('3');
      expect(args).not.toContain('-A');
      expect(args).not.toContain('-B');
    });

    it('falls back to -A/-B when contextLines is absent', () => {
      const args = buildArgs({ beforeContext: 2, afterContext: 4 });
      expect(args[args.indexOf('-B') + 1]).toBe('2');
      expect(args[args.indexOf('-A') + 1]).toBe('4');
      expect(args).not.toContain('-C');
    });

    it('switches output modes off --json for plain-text flags', () => {
      expect(buildArgs({ filesOnly: true })).toContain('-l');
      expect(buildArgs({ filesOnly: true })).not.toContain('--json');
      expect(buildArgs({ count: true })).toContain('-c');
      expect(buildArgs({ count: true })).not.toContain('--json');
      expect(buildArgs({ countMatches: true })).toContain('--count-matches');
    });

    it('consolidates simple extension globs into a brace pattern', () => {
      const args = buildArgs({ include: ['*.ts', '*.js'] });
      expect(args).toContain('*.{ts,js}');
    });

    it('negates exclude and excludeDir globs', () => {
      const args = buildArgs({
        exclude: ['*.lock'],
        excludeDir: ['node_modules'],
      });
      expect(args).toContain('!*.lock');
      expect(args).toContain('!node_modules/');
    });

    it('emits --sortr when sortReverse is set', () => {
      const args = buildArgs({ sortReverse: true, sort: 'modified' });
      expect(args[args.indexOf('--sortr') + 1]).toBe('modified');
      expect(args).not.toContain('--sort');
    });

    it('passes through threads, multiline, hidden and no-ignore flags', () => {
      const args = buildArgs({
        threads: 4,
        multiline: true,
        multilineDotall: true,
        hidden: true,
        noIgnore: true,
      });
      expect(args[args.indexOf('-j') + 1]).toBe('4');
      expect(args).toContain('-U');
      expect(args).toContain('--multiline-dotall');
      expect(args).toContain('--hidden');
      expect(args).toContain('--no-ignore');
    });

    it('produces a stable full vector for a comprehensive query', () => {
      const args = buildArgs({
        pattern: 'needle',
        path: '/src',
        fixedString: true,
        caseSensitive: true,
        wholeWord: true,
        contextLines: 2,
        include: ['*.ts'],
        excludeDir: ['dist'],
        hidden: true,
        threads: 8,
        includeStats: true,
        maxMatchesPerFile: 5,
      });
      expect(args).toMatchInlineSnapshot(`
        [
          "-F",
          "-s",
          "-w",
          "-C",
          "2",
          "-n",
          "--column",
          "-m",
          "5",
          "-g",
          "*.ts",
          "-g",
          "!dist/",
          "--hidden",
          "--json",
          "-j",
          "4",
          "--stats",
          "--sort",
          "path",
          "--color",
          "never",
          "--",
          "needle",
          "/src",
        ]
      `);
    });
  });
});
