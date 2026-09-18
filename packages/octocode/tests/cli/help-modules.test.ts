import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { findCommandSpec } from '../../src/cli/commands/specs.js';
import { showCommandHelp } from '../../src/cli/help.js';

describe('Node-owned command help', () => {
  let stdout: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    stdout = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
  });

  afterEach(() => stdout.mockRestore());

  it('renders the skill contract without native command duplication', () => {
    showCommandHelp(findCommandSpec('skill')!);
    const output = stdout.mock.calls
      .map((call: unknown[]) => String(call[0]))
      .join('');
    expect(output).toContain('octocode skill');
    expect(output).toContain('USAGE');
    expect(output).toContain('OPTIONS');
    expect(output).toContain('--platform');
    expect(output).not.toContain('localSearch');
  });

  it('does not define help for native-owned commands', () => {
    for (const name of ['install', 'tools', 'context', 'status']) {
      expect(findCommandSpec(name)).toBeUndefined();
    }
  });
});
