import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// Keep color helpers as identity passthroughs so assertions match plain text.
vi.mock('../../src/utils/colors.js', () => ({
  c: (_color: string, s: string) => s,
  bold: (s: string) => s,
  dim: (s: string) => s,
}));

import {
  findUnknownOptions,
  getAllowedOptionNames,
  findInvalidNumericOptions,
  printUnknownOptionError,
} from '../../src/cli/command-validation.js';
import type { CLICommand, ParsedArgs } from '../../src/cli/types.js';

function args(
  options: ParsedArgs['options'],
  command: ParsedArgs['command'] = null
): ParsedArgs {
  return { command, args: [], options };
}

// Synthetic command with a representative option set (a numeric-looking flag
// for typo-suggestion, plus canonical spellings that must be accepted) — kept
// independent of any real command so this suite never breaks when commands
// are added/renamed/removed.
const fixtureCommand: CLICommand = {
  name: 'fixture',
  options: [
    { name: 'depth', hasValue: true },
    { name: 'content-view', hasValue: true },
    { name: 'items-per-page', hasValue: true },
  ],
  handler: () => {},
};

describe('command option validation', () => {
  it('accepts a command-declared flag', () => {
    expect(findUnknownOptions(fixtureCommand, args({ depth: '2' }))).toEqual(
      []
    );
  });

  it('accepts global flags on any command', () => {
    expect(
      findUnknownOptions(fixtureCommand, args({ json: true, 'no-color': true }))
    ).toEqual([]);
  });

  it('flags an unknown option', () => {
    expect(findUnknownOptions(fixtureCommand, args({ dpeth: '2' }))).toEqual([
      'dpeth',
    ]);
  });

  it('rejects removed aliases from command metadata too', () => {
    expect(
      findUnknownOptions(
        fixtureCommand,
        args({ mode: 'none', 'page-size': '5', type: 'ts' }, 'fixture')
      )
    ).toEqual(['mode', 'page-size', 'type']);
  });

  it('accepts canonical spelling for content and pagination', () => {
    expect(
      findUnknownOptions(
        fixtureCommand,
        args({ 'content-view': 'none', 'items-per-page': '5' }, 'fixture')
      )
    ).toEqual([]);
  });

  it('always allows the global flag set', () => {
    const allowed = getAllowedOptionNames(fixtureCommand);
    for (const g of ['json', 'compact', 'no-color', 'help', 'version']) {
      expect(allowed.has(g)).toBe(true);
    }
  });
});

describe('findInvalidNumericOptions', () => {
  it('flags a non-integer numeric value', () => {
    expect(findInvalidNumericOptions(args({ limit: 'abc' }))).toEqual([
      '--limit=abc',
    ]);
  });

  it('does not recognize the removed --context alias as numeric', () => {
    expect(findInvalidNumericOptions(args({ context: 'abc' }))).toEqual([]);
  });

  it('validates --items-per-page as numeric', () => {
    expect(
      findInvalidNumericOptions(args({ 'items-per-page': 'abc' }, 'fixture'))
    ).toEqual(['--items-per-page=abc']);
  });

  it('flags a negative numeric value', () => {
    expect(findInvalidNumericOptions(args({ page: '-1' }))).toEqual([
      '--page=-1',
    ]);
  });

  it('flags a value with trailing junk', () => {
    expect(findInvalidNumericOptions(args({ depth: '3x' }))).toEqual([
      '--depth=3x',
    ]);
  });

  it('allows cache --depth enum values', () => {
    expect(findInvalidNumericOptions(args({ depth: 'tree' }, 'cache'))).toEqual(
      []
    );
  });

  it('validates --match-length as numeric', () => {
    expect(findInvalidNumericOptions(args({ 'match-length': 'abc' }))).toEqual([
      '--match-length=abc',
    ]);
  });

  it('accepts valid non-negative integers', () => {
    expect(findInvalidNumericOptions(args({ limit: '10', page: '0' }))).toEqual(
      []
    );
  });

  it('ignores non-numeric flags and boolean values', () => {
    expect(
      findInvalidNumericOptions(args({ mode: 'abc', json: true }))
    ).toEqual([]);
  });
});

describe('printUnknownOptionError', () => {
  let logSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    logSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
  });

  afterEach(() => {
    logSpy.mockRestore();
  });

  function output(): string {
    return logSpy.mock.calls
      .map((call: unknown[]) => call.join(' '))
      .join('\n');
  }

  it('names the offending flag and lists valid flags', () => {
    printUnknownOptionError(fixtureCommand, ['bogus']);
    const out = output();
    expect(out).toContain(`Unknown flag --bogus for '${fixtureCommand.name}'`);
    expect(out).toContain(`Valid flags for ${fixtureCommand.name}`);
  });

  it('suggests a near-miss flag for a typo', () => {
    printUnknownOptionError(fixtureCommand, ['depht']);
    expect(output()).toContain('did you mean --depth?');
  });

  it('does not suggest anything for an unrelated flag', () => {
    printUnknownOptionError(fixtureCommand, ['xxxxxxxxxx']);
    expect(output()).not.toContain('did you mean');
  });
});
