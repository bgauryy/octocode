import { describe, it, expect } from 'vitest';
import { readFileSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { REGISTERED_COMMAND_NAMES } from '../../src/cli/commands/index.js';

/**
 * Guards that the Octocode skill doc's quick-command examples reference only
 * commands the CLI actually registers — so a removed/renamed command (e.g. the
 * nonexistent `ast`) can't linger in the docs.
 */
const SKILL_PATH = join(
  dirname(fileURLToPath(import.meta.url)),
  '../../../../.agents/skills/octocode/SKILL.md'
);

describe('skill doc command examples', () => {
  it('every `octocode.js <cmd>` example uses a registered command', () => {
    if (!existsSync(SKILL_PATH)) return; // skill doc not present in this checkout
    const doc = readFileSync(SKILL_PATH, 'utf8');
    const allowed = new Set<string>([
      ...REGISTERED_COMMAND_NAMES,
      'tools',
      'context',
    ]);
    const invocations = [...doc.matchAll(/octocode\.js\s+([a-z-]+)/g)]
      .map(m => m[1]!)
      .filter(cmd => !cmd.startsWith('-'));
    const unknown = invocations.filter(cmd => !allowed.has(cmd));
    expect(unknown).toEqual([]);
  });

  it('does not advertise a standalone `ast` command (structural is search --pattern/--rule)', () => {
    if (!existsSync(SKILL_PATH)) return;
    const doc = readFileSync(SKILL_PATH, 'utf8');
    expect(REGISTERED_COMMAND_NAMES).not.toContain('ast');
    // no `ast '<...>'` quick-command form
    expect(/`ast\s+'/.test(doc)).toBe(false);
  });
});
