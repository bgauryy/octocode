import { describe, expect, it } from 'vitest';

import { RipgrepCommandBuilder } from '../../src/commands/RipgrepCommandBuilder.js';
import type { RipgrepQuery } from '../../src/tools/local_ripgrep/scheme.js';

describe('RipgrepCommandBuilder', () => {
  it('builds deterministic JSON ripgrep args from a rich query', () => {
    const query: RipgrepQuery = {
      keywords: 'needle',
      path: 'src',
      fixedString: true,
      caseInsensitive: true,
      contextLines: 2,
      include: ['*.ts', '*.tsx', 'src/**/*.js'],
      exclude: ['dist/**'],
      excludeDir: ['node_modules'],
      hidden: true,
      noIgnore: true,
      multiline: true,
      multilineDotall: true,
      sort: 'modified',
      sortReverse: true,
      langType: 'ts',
    };

    const { command, args } = new RipgrepCommandBuilder()
      .fromQuery(query)
      .build();

    expect(command).toBeTruthy();
    expect(args).toEqual(
      expect.arrayContaining([
        '-F',
        '-i',
        '-C',
        '2',
        '--json',
        '-t',
        'ts',
        '-g',
        '*.{ts,tsx}',
        '-g',
        'src/**/*.js',
        '-g',
        '!dist/**',
        '-g',
        '!node_modules/',
        '--hidden',
        '--no-ignore',
        '-U',
        '--multiline-dotall',
        '--sortr',
        'modified',
        '--color',
        'never',
      ])
    );
    expect(args).not.toContain('--sort');
    expect(args.slice(args.indexOf('--') + 1)).toEqual(['needle', 'src']);
  });

  it('uses plain-text ripgrep output for files-only mode', () => {
    const args = new RipgrepCommandBuilder()
      .fromQuery({
        keywords: 'needle',
        path: 'src',
        filesOnly: true,
      })
      .getArgs();

    expect(args).toContain('-l');
    expect(args).not.toContain('--json');
  });
});
