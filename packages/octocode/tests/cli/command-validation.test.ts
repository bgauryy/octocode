import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('../../src/utils/colors.js', () => ({
  c: (_color: string, value: string) => value,
  bold: (value: string) => value,
  dim: (value: string) => value,
}));

import {
  findUnknownOptions,
  getAllowedOptionNames,
  printUnknownOptionError,
} from '../../src/cli/command-validation.js';
import type { CLICommand, ParsedArgs } from '../../src/cli/types.js';

const command: CLICommand = {
  name: 'fixture',
  options: [{ name: 'depth', hasValue: true }],
  handler: () => {},
};
const args = (options: ParsedArgs['options']): ParsedArgs => ({
  command: 'fixture',
  args: [],
  options,
});

describe('Node command option validation', () => {
  it('accepts runtime and global flags while rejecting unknown fields', () => {
    expect(
      findUnknownOptions(command, args({ depth: '2', json: true }))
    ).toEqual([]);
    expect(findUnknownOptions(command, args({ depht: '2' }))).toEqual([
      'depht',
    ]);
    expect(getAllowedOptionNames(command).has('depth')).toBe(true);
    expect(getAllowedOptionNames(command).has('help')).toBe(true);
  });

  describe('diagnostic', () => {
    let log: ReturnType<typeof vi.spyOn>;
    beforeEach(() => {
      log = vi.spyOn(console, 'log').mockImplementation(() => {});
    });
    afterEach(() => log.mockRestore());

    it('suggests the nearest declared flag', () => {
      printUnknownOptionError(command, ['depht']);
      const output = log.mock.calls
        .map((call: unknown[]) => call.join(' '))
        .join('\n');
      expect(output).toContain('Unknown flag --depht');
      expect(output).toContain('did you mean --depth?');
    });
  });
});
