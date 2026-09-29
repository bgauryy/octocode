import { describe, expect, it, vi } from 'vitest';

vi.mock('node:os', async importOriginal => ({
  ...(await importOriginal<typeof import('node:os')>()),
  homedir: () => '/home/tester',
}));

import { shortPath } from '../../../../src/cli/commands/skills/utils/paths.js';

describe('shortPath', () => {
  it('replaces only a leading home prefix', () => {
    expect(shortPath('/home/tester/.claude/skills/x')).toBe(
      '~/.claude/skills/x'
    );
    expect(shortPath('/home/tester')).toBe('~');
  });

  it('does not touch HOME occurrences elsewhere or sibling prefixes', () => {
    expect(shortPath('/mnt/backup/home/tester/x')).toBe(
      '/mnt/backup/home/tester/x'
    );
    expect(shortPath('/home/tester2/x')).toBe('/home/tester2/x');
  });
});
