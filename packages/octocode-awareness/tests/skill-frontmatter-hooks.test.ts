import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/**
 * Claude Code only substitutes `${CLAUDE_SKILL_DIR}` inside skill/agent
 * frontmatter (requires Claude Code v2.1.196+); there is no bare `$SKILL_DIR`
 * or `${SKILL_DIR}` variable. A hook `command:` using either silently resolves
 * to a nonexistent path, so the hook never runs — verified against
 * https://code.claude.com/docs/en/skills and .../hooks. This regressed once
 * (all 6 lifecycle hooks in SKILL.md used `$SKILL_DIR`); this test pins it.
 */

const TEST_DIR = dirname(fileURLToPath(import.meta.url));
const SKILL_MD = resolve(TEST_DIR, '../skills/octocode-awareness/SKILL.md');

function frontmatter(path: string): string {
  const text = readFileSync(path, 'utf8');
  const match = /^---\n([\s\S]*?)\n---/.exec(text);
  if (!match) throw new Error(`no frontmatter block in ${path}`);
  return match[1]!;
}

describe('SKILL.md hook frontmatter', () => {
  it('never references the nonexistent $SKILL_DIR / ${SKILL_DIR} variable', () => {
    const fm = frontmatter(SKILL_MD);
    expect(fm).not.toMatch(/\$SKILL_DIR\b/);
    expect(fm).not.toMatch(/\$\{SKILL_DIR\}/);
  });

  it('uses ${CLAUDE_SKILL_DIR} for every bundled hook script path', () => {
    const fm = frontmatter(SKILL_MD);
    const commands = [...fm.matchAll(/command\s*:\s*"([^"]+)"/g)].map((m) => m[1]!);
    expect(commands.length).toBeGreaterThan(0);
    for (const command of commands) {
      if (command.includes('scripts/hooks/')) {
        expect(command.startsWith('${CLAUDE_SKILL_DIR}/')).toBe(true);
      }
    }
  });
});
