import { describe, expect, it } from 'vitest';

import {
  REGISTERED_COMMAND_NAMES,
  isRegisteredCommand,
  loadCommand,
} from '../../src/cli/commands/index.js';

describe('Node command registry', () => {
  it('contains only skill materialization', async () => {
    expect(REGISTERED_COMMAND_NAMES).toEqual(['skill']);
    expect(isRegisteredCommand('skill')).toBe(true);
    expect((await loadCommand('skill'))?.name).toBe('skill');
  });

  it('does not duplicate native commands', async () => {
    for (const name of [
      'tools',
      'context',
      'install',
      'status',
      'login',
      'logout',
      'lsp-server',
    ]) {
      expect(isRegisteredCommand(name)).toBe(false);
      expect(await loadCommand(name)).toBeUndefined();
    }
  });
});
