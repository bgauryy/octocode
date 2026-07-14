import { describe, it, expect } from 'vitest';

describe('CLI command registry', () => {
  it('does not expose removed commands', async () => {
    const { findCommand, loadCommand } =
      await import('../../src/cli/commands/index.js');

    const removed = [
      'token',
      'skills',
      'cat',
      'ls',
      'find',
      'diff',
      'history',
      'repo',
      'pkg',
      'binary',
      'unzip',
      'grep',
      'lsp',
    ];
    for (const name of removed) {
      expect(findCommand(name)).toBeUndefined();
      expect(await loadCommand(name)).toBeUndefined();
    }
  });

  it('keeps status as the read-only token/auth command', async () => {
    const { findCommand } = await import('../../src/cli/commands/index.js');
    const cmd = findCommand('status');

    expect(cmd).toBeDefined();
    expect(cmd!.name).toBe('status');
  });
});
