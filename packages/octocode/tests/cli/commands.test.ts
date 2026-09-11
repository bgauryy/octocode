import { describe, it, expect } from 'vitest';

describe('CLI command registry', () => {
  it('does not expose removed commands', async () => {
    const { isRegisteredCommand, loadCommand } =
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
      expect(isRegisteredCommand(name)).toBe(false);
      expect(await loadCommand(name)).toBeUndefined();
    }
  });

  it('keeps status as the read-only token/auth command', async () => {
    const { isRegisteredCommand, loadCommand } =
      await import('../../src/cli/commands/index.js');
    const cmd = await loadCommand('status');

    expect(isRegisteredCommand('status')).toBe(true);
    expect(cmd).toBeDefined();
    expect(cmd!.name).toBe('status');
  });
});
