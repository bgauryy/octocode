import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const PACKAGE_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SCRIPT = resolve(PACKAGE_ROOT, 'skills/octocode-awareness/scripts/install.mjs');

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
      next: 'Run npx @octocodeai/octocode-awareness init --compact once, then attend --compact.',
    });
    expect(parsed.optional_skill_count).toEqual(expect.any(Number));
  });
});
