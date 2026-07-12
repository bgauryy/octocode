import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const SCRIPT = resolve(import.meta.dirname, '../../../skills/octocode-awareness/scripts/install.mjs');

describe('skill install diagnosis', () => {
  it('prints a bounded compact readiness receipt', () => {
    const result = spawnSync(process.execPath, [SCRIPT, '--compact'], { encoding: 'utf8', timeout: 30_000 });
    expect(result.status, result.stderr || result.stdout).toBe(0);
    expect(result.stderr).not.toContain('ExperimentalWarning');
    expect(Buffer.byteLength(result.stdout, 'utf8')).toBeLessThanOrEqual(256);
    const parsed = JSON.parse(result.stdout) as Record<string, unknown>;
    expect(Object.keys(parsed).sort()).toEqual(['next', 'ok', 'optional_skill_count', 'required_skills']);
    expect(parsed).toMatchObject({
      ok: true,
      required_skills: ['octocode-awareness'],
      next: 'Run maintenance init once, then attend --compact.',
    });
    expect(parsed.optional_skill_count).toEqual(expect.any(Number));
  });
});
