import path from 'node:path';
import { describe, expect, it } from 'vitest';

import { skillDiscoveryRoots } from '../src/tools/skill-tool.js';

describe('Pi adapter Skill compatibility roots', () => {
  it('includes Pi, Claude, Cursor, Codex, .agent, and .agents in project and user scopes', () => {
    const cwd = path.resolve('/workspace');
    const home = path.resolve('/home/user');
    const roots = skillDiscoveryRoots(cwd, home);

    for (const relative of [
      '.pi/skills',
      '.pi/agent/skills',
      '.claude/skills',
      '.cursor/skills',
      '.codex/skills',
      '.agent/skills',
      '.agents/skills',
    ]) {
      expect(roots).toContainEqual(expect.objectContaining({ dir: path.join(cwd, relative) }));
      expect(roots).toContainEqual(expect.objectContaining({ dir: path.join(home, relative) }));
    }
  });
});
