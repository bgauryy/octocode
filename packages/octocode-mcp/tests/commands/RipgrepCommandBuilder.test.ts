import { describe, it, expect } from 'vitest';
import { RipgrepCommandBuilder } from '../../src/commands/RipgrepCommandBuilder.js';
import { RipgrepQuerySchema } from '@octocodeai/octocode-core';

const createQuery = (query: Record<string, unknown>) =>
  RipgrepQuerySchema.parse({
    id: 'test:rg',
    researchGoal: 'Test',
    reasoning: 'Schema validation',
    ...query,
  });

describe('RipgrepCommandBuilder', () => {
  describe('basic command building', () => {
    it('should build a simple search command', () => {
      const builder = new RipgrepCommandBuilder();
      const { command, args } = builder.simple('pattern', '/path').build();

      // T3.3 — command is now the bundled @vscode/ripgrep absolute
      // path (or 'rg' fallback). Either way it ends in 'rg'.
      expect(command).toMatch(/rg$/);
      expect(args).toContain('-n'); // Line numbers
      expect(args).toContain('-S'); // Smart case
      expect(args).toContain('pattern');
      expect(args).toContain('/path');
    });

    it('should include default optimizations', () => {
      const builder = new RipgrepCommandBuilder();
      const { args } = builder.simple('test', '/repo').build();

      expect(args).toContain('--sort'); // Sorting enabled
      expect(args).toContain('path');
      expect(args).toContain('--color');
      expect(args).toContain('never');
    });

    it('should insert -- before positional args to prevent option injection', () => {
      const builder = new RipgrepCommandBuilder();
      const { args } = builder.simple('--pre=cat', '/repo').build();

      const separatorIndex = args.indexOf('--');
      expect(separatorIndex).toBeGreaterThan(-1);
      expect(args[separatorIndex + 1]).toBe('--pre=cat');
      expect(args[separatorIndex + 2]).toBe('/repo');
    });
  });

  describe('pattern modes', () => {
    it('should use fixed string mode', () => {
      const query = createQuery({
        pattern: 'literal.string',
        path: './src',
        fixedString: true,
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('-F');
    });

    it('should use PCRE2 mode', () => {
      const query = createQuery({
        pattern: '(?<=export )\\w+',
        path: './src',
        perlRegex: true,
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('-P');
    });
  });

  describe('case sensitivity', () => {
    it('should use smart case by default', () => {
      const query = createQuery({
        pattern: 'function',
        path: './src',
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('-S');
    });

    it('should override with case insensitive', () => {
      const query = createQuery({
        pattern: 'FUNCTION',
        path: './src',
        caseInsensitive: true,
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('-i');
      expect(args).not.toContain('-S');
    });
  });

  describe('file filtering', () => {
    it('should use type filtering', () => {
      const query = createQuery({
        pattern: 'import',
        path: './src',
        type: 'ts',
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('-t');
      expect(args).toContain('ts');
    });

    it('should consolidate simple globs (optimization)', () => {
      const query = createQuery({
        pattern: 'export',
        path: './src',
        include: ['*.ts', '*.tsx'],
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      // Should consolidate into single glob
      const globIndex = args.indexOf('-g');
      expect(globIndex).not.toBe(-1);

      const glob = args[globIndex + 1];
      expect(glob).toBe('*.{ts,tsx}');
    });

    it('should handle complex globs separately', () => {
      const query = createQuery({
        pattern: 'test',
        path: './src',
        include: ['*.ts', 'src/**/*.test.*'],
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      // Should have both globs
      const firstGlobIndex = args.indexOf('-g');
      expect(firstGlobIndex).not.toBe(-1);

      // Check for both globs
      expect(args.filter(a => a === '-g').length).toBeGreaterThanOrEqual(1);
    });

    it('should handle exclude patterns', () => {
      const query = createQuery({
        pattern: 'function',
        path: './src',
        exclude: ['*.test.*', '*.spec.*'],
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('-g');
      expect(args).toContain('!*.test.*');
      expect(args).toContain('!*.spec.*');
    });

    it('should use fixedString when pattern has regex special chars (e.g. describe()', () => {
      // describe( has ( which is regex special - unclosed group without fixedString
      const query = createQuery({
        pattern: 'describe(',
        path: './src',
        exclude: ['*.test.ts'],
        fixedString: true,
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('-F');
      expect(args).toContain('-g');
      expect(args).toContain('!*.test.ts');
      expect(args).toContain('describe(');
    });

    it('should handle excludeDir', () => {
      const query = createQuery({
        pattern: 'import',
        path: './src',
        excludeDir: ['node_modules', 'dist'],
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('!node_modules/');
      expect(args).toContain('!dist/');
    });
  });

  describe('output control', () => {
    it('should use filesOnly mode', () => {
      const query = createQuery({
        pattern: 'auth',
        path: './src',
        filesOnly: true,
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('-l');
    });

    it('should apply default maxMatchesPerFile', () => {
      const query = createQuery({
        pattern: 'function',
        path: './src',
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('-m');
      expect(args).toContain('10'); // Default (matchesPerPage)
    });

    it('should not apply maxMatches in filesOnly mode', () => {
      const query = createQuery({
        pattern: 'function',
        path: './src',
        filesOnly: true,
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      // Should have -l but not -m
      expect(args).toContain('-l');
      const mIndex = args.indexOf('-m');
      expect(mIndex).toBe(-1);
    });

    it('should use custom maxMatchesPerFile', () => {
      const query = createQuery({
        pattern: 'export',
        path: './src',
        maxMatchesPerFile: 10,
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('-m');
      expect(args).toContain('10');
    });
  });

  describe('context lines', () => {
    it('should add context lines', () => {
      const query = createQuery({
        pattern: 'error',
        path: './logs',
        contextLines: 5,
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('-C');
      expect(args).toContain('5');
    });

    it('should handle before/after context separately', () => {
      const query = createQuery({
        pattern: 'error',
        path: './logs',
        beforeContext: 3,
        afterContext: 2,
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('-B');
      expect(args).toContain('3');
      expect(args).toContain('-A');
      expect(args).toContain('2');
    });
  });

  describe('advanced features', () => {
    it('should enable multiline mode', () => {
      const query = createQuery({
        pattern: 'async.*\\n.*await',
        path: './src',
        multiline: true,
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('-U');
    });

    it('should enable stats', () => {
      const query = createQuery({
        pattern: 'function',
        path: './src',
        includeStats: true,
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('--stats');
    });

    it('should NOT include --stats when filesOnly is true even if includeStats is true', () => {
      const query = createQuery({
        pattern: 'function',
        path: './src',
        includeStats: true,
        filesOnly: true,
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).not.toContain('--stats');
      expect(args).toContain('-l');
    });

    it('should NOT include --stats when filesWithoutMatch is true even if includeStats is true', () => {
      const query = createQuery({
        pattern: 'function',
        path: './src',
        includeStats: true,
        filesWithoutMatch: true,
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).not.toContain('--stats');
      expect(args).toContain('--files-without-match');
    });

    it('should enable JSON output', () => {
      const query = createQuery({
        pattern: 'export',
        path: './src',
        jsonOutput: true,
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('--json');
    });
  });

  describe('sorting', () => {
    it('should sort by path by default', () => {
      const query = createQuery({
        pattern: 'test',
        path: './src',
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('--sort');
      expect(args).toContain('path');
    });

    it('should sort by modified time', () => {
      const query = createQuery({
        pattern: 'FIXME',
        path: './src',
        sort: 'modified',
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('--sort');
      expect(args).toContain('modified');
    });

    it('should reverse sort', () => {
      const query = createQuery({
        pattern: 'TODO',
        path: './src',
        sort: 'path',
        sortReverse: true,
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('--sortr');
      expect(args).toContain('path');
      expect(args).not.toContain('--sort');
    });

    it('should properly handle sort option removal when switching', () => {
      // Test that --sort is removed when switching to --sortr
      const queryReverse = createQuery({
        pattern: 'test',
        path: './src',
        sort: 'modified',
        sortReverse: true,
      });

      const { args: argsReverse } = new RipgrepCommandBuilder()
        .fromQuery(queryReverse)
        .build();

      // Should have --sortr, not --sort
      const sortIndex = argsReverse.indexOf('--sort');
      const sortrIndex = argsReverse.indexOf('--sortr');

      expect(sortrIndex).not.toBe(-1);
      expect(sortIndex).toBe(-1); // --sort should not exist

      // Test that --sortr is removed when switching to --sort
      const queryNormal = createQuery({
        pattern: 'test',
        path: './src',
        sort: 'modified',
        sortReverse: false,
      });

      const { args: argsNormal } = new RipgrepCommandBuilder()
        .fromQuery(queryNormal)
        .build();

      // Should have --sort, not --sortr
      const sortIndex2 = argsNormal.indexOf('--sort');
      const sortrIndex2 = argsNormal.indexOf('--sortr');

      expect(sortIndex2).not.toBe(-1);
      expect(sortrIndex2).toBe(-1); // --sortr should not exist
    });
  });

  describe('glob consolidation', () => {
    it('should consolidate 3+ simple globs', () => {
      const query = createQuery({
        pattern: 'import',
        path: './src',
        include: ['*.ts', '*.tsx', '*.js', '*.jsx'],
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      const globIndex = args.indexOf('-g');
      expect(globIndex).not.toBe(-1);

      const glob = args[globIndex + 1];
      expect(glob).toBe('*.{ts,tsx,js,jsx}');
    });

    it('should not consolidate already-consolidated globs', () => {
      const query = createQuery({
        pattern: 'test',
        path: './src',
        include: ['*.{ts,tsx}'],
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      const globIndex = args.indexOf('-g');
      expect(globIndex).not.toBe(-1);

      const glob = args[globIndex + 1];
      expect(glob).toBe('*.{ts,tsx}');
    });
  });

  describe('complete query', () => {
    it('should build a complex query with all features', () => {
      const query = createQuery({
        pattern: '(login|logout|session)',
        path: '/repo/src',
        type: 'ts',
        contextLines: 3,
        maxMatchesPerFile: 5,
        smartCase: true,
        excludeDir: ['node_modules', '__tests__'],
        includeStats: true,
        sort: 'modified',
      });

      const { command, args } = new RipgrepCommandBuilder()
        .fromQuery(query)
        .build();

      expect(command).toMatch(/rg$/);

      // Pattern mode
      expect(args).toContain('-S'); // Smart case

      // File filtering
      expect(args).toContain('-t');
      expect(args).toContain('ts');
      expect(args).toContain('!node_modules/');
      expect(args).toContain('!__tests__/');

      // Output control
      expect(args).toContain('-C');
      expect(args).toContain('3');
      expect(args).toContain('-m');
      expect(args).toContain('5');

      // Advanced
      expect(args).toContain('--stats');
      expect(args).toContain('--sort');
      expect(args).toContain('modified');

      // Pattern and path
      expect(args).toContain('(login|logout|session)');
      expect(args).toContain('/repo/src');
    });
  });

  describe('count modes', () => {
    it('should use count mode with -c flag', () => {
      const query = createQuery({
        pattern: 'TODO',
        path: './src',
        count: true,
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('-c');
    });

    it('should use countMatches mode with --count-matches flag', () => {
      const query = createQuery({
        pattern: 'FIXME',
        path: './src',
        countMatches: true,
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('--count-matches');
    });

    it('should use filesWithoutMatch mode', () => {
      const query = createQuery({
        pattern: 'deprecated',
        path: './src',
        filesWithoutMatch: true,
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('--files-without-match');
    });
  });

  describe('additional flags', () => {
    it('should add noIgnore flag', () => {
      const query = createQuery({
        pattern: 'test',
        path: './src',
        noIgnore: true,
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('--no-ignore');
    });

    it('should add hidden flag', () => {
      const query = createQuery({
        pattern: 'config',
        path: './',
        hidden: true,
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('--hidden');
    });

    it('should add multilineDotall flag when multiline is enabled', () => {
      const query = createQuery({
        pattern: 'start.*end',
        path: './src',
        multiline: true,
        multilineDotall: true,
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('-U');
      expect(args).toContain('--multiline-dotall');
    });

    it('should not add multilineDotall without multiline', () => {
      const query = createQuery({
        pattern: 'test',
        path: './src',
        multilineDotall: true, // Should be ignored without multiline: true
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).not.toContain('--multiline-dotall');
    });

    it('should add threads option', () => {
      const query = createQuery({
        pattern: 'search',
        path: './src',
        threads: 4,
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('-j');
      expect(args).toContain('4');
    });

    it('should add no-mmap flag when mmap is false', () => {
      const query = createQuery({
        pattern: 'test',
        path: './src',
        mmap: false,
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('--no-mmap');
    });

    it('should add noMessages flag', () => {
      const query = createQuery({
        pattern: 'test',
        path: './src',
        noMessages: true,
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('--no-messages');
    });

    it('should add lineRegexp flag', () => {
      const query = createQuery({
        pattern: 'exact-line-match',
        path: './src',
        lineRegexp: true,
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('-x');
    });

    it('should add passthru flag', () => {
      const query = createQuery({
        pattern: 'highlight',
        path: './src',
        passthru: true,
      });

      const { args } = new RipgrepCommandBuilder().fromQuery(query).build();

      expect(args).toContain('--passthru');
    });
  });

  describe('fluent API methods', () => {
    it('should chain smartCase method', () => {
      const builder = new RipgrepCommandBuilder();
      const { args } = builder.smartCase().build();

      expect(args).toContain('-S');
    });

    it('should chain filesOnly method', () => {
      const builder = new RipgrepCommandBuilder();
      const { args } = builder.filesOnly().build();

      expect(args).toContain('-l');
    });

    it('should chain context method', () => {
      const builder = new RipgrepCommandBuilder();
      const { args } = builder.context(10).build();

      expect(args).toContain('-C');
      expect(args).toContain('10');
    });

    it('should chain include method', () => {
      const builder = new RipgrepCommandBuilder();
      const { args } = builder.include('*.ts').build();

      expect(args).toContain('-g');
      expect(args).toContain('*.ts');
    });

    it('should chain exclude method', () => {
      const builder = new RipgrepCommandBuilder();
      const { args } = builder.exclude('*.test.ts').build();

      expect(args).toContain('-g');
      expect(args).toContain('!*.test.ts');
    });

    it('should chain excludeDir method', () => {
      const builder = new RipgrepCommandBuilder();
      const { args } = builder.excludeDir('node_modules').build();

      expect(args).toContain('-g');
      expect(args).toContain('!node_modules/');
    });

    it('should chain type method', () => {
      const builder = new RipgrepCommandBuilder();
      const { args } = builder.type('js').build();

      expect(args).toContain('-t');
      expect(args).toContain('js');
    });

    it('should chain fixedString method', () => {
      const builder = new RipgrepCommandBuilder();
      const { args } = builder.fixedString().build();

      expect(args).toContain('-F');
    });

    it('should chain perlRegex method', () => {
      const builder = new RipgrepCommandBuilder();
      const { args } = builder.perlRegex().build();

      expect(args).toContain('-P');
    });

    it('should chain maxMatches method', () => {
      const builder = new RipgrepCommandBuilder();
      const { args } = builder.maxMatches(50).build();

      expect(args).toContain('-m');
      expect(args).toContain('50');
    });

    it('should chain multiple methods together', () => {
      const builder = new RipgrepCommandBuilder();
      const { args } = builder
        .smartCase()
        .type('ts')
        .include('*.tsx')
        .excludeDir('dist')
        .context(5)
        .maxMatches(20)
        .build();

      expect(args).toContain('-S');
      expect(args).toContain('-t');
      expect(args).toContain('ts');
      expect(args).toContain('*.tsx');
      expect(args).toContain('!dist/');
      expect(args).toContain('-C');
      expect(args).toContain('5');
      expect(args).toContain('-m');
      expect(args).toContain('20');
    });
  });

  describe('sort option clearing', () => {
    it('should clear --sort when switching to --sortr via query', () => {
      // First create a builder with --sort set (default)
      const builder = new RipgrepCommandBuilder();

      // Build with sortReverse to trigger clearSortOption
      const query = createQuery({
        pattern: 'test',
        path: './src',
        sortReverse: true,
      });

      const { args } = builder.fromQuery(query).build();

      // --sortr should be present, --sort should not
      expect(args).toContain('--sortr');
      expect(args.indexOf('--sort')).toBe(-1);
    });

    it('should clear --sortr when switching to --sort via query', () => {
      // Create a query that first sets --sortr then switches to --sort
      const builder = new RipgrepCommandBuilder();

      // First apply a reverse sort
      const queryReverse = createQuery({
        pattern: 'test',
        path: './src',
        sortReverse: true,
      });
      builder.fromQuery(queryReverse);

      // Now apply a normal sort (should clear --sortr)
      const queryNormal = createQuery({
        pattern: 'test2',
        path: './src',
        sortReverse: false,
      });

      const { args } = builder.fromQuery(queryNormal).build();

      // --sort should be present, --sortr should not
      expect(args).toContain('--sort');
      expect(args.indexOf('--sortr')).toBe(-1);
    });
  });
});
