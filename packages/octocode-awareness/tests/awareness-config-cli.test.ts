import { describe, expect, it } from 'vitest';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { join, resolve, dirname } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';

const PACKAGE_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SCRIPT = resolve(PACKAGE_ROOT, 'out/octocode-awareness.js');

function run(home: string, args: string[]) {
  const result = spawnSync(process.execPath, [SCRIPT, ...args, '--compact'], {
    encoding: 'utf8',
    timeout: 5_000,
    env: { ...process.env, OCTOCODE_HOME: home, OCTOCODE_AGENT_DIR: join(home, 'memory') },
  });
  return {
    status: result.status,
    payload: JSON.parse(result.stdout) as Record<string, unknown>,
  };
}

describe('awareness config CLI', () => {
  it('requires all user answers before creating awareness.json', () => {
    const home = mkdtempSync(join(tmpdir(), 'awareness-config-cli-'));
    try {
      const shown = run(home, ['config', 'show']);
      expect(shown.status).toBe(0);
      expect(shown.payload).toMatchObject({ exists: false, source: 'defaults', requires_user_answers: true });
      expect(shown.payload.questions).toHaveLength(5);

      const incomplete = run(home, ['config', 'init', '--hooks', 'true']);
      expect(incomplete.status).toBe(1);
      expect(incomplete.payload).toMatchObject({ ok: false, requires_user_answers: true });

      const created = run(home, [
        'config', 'init',
        '--hooks', 'true',
        '--notifications', 'false',
        '--verification-gate', 'true',
        '--session-capture', 'false',
        '--maintenance-reminders', 'true',
      ]);
      expect(created.status).toBe(0);
      expect(created.payload).toMatchObject({ ok: true, action: 'init', created: true });
      expect(JSON.parse(readFileSync(join(home, 'awareness.json'), 'utf8'))).toMatchObject({
        version: 1,
        features: {
          hooks: true,
          notifications: false,
          verificationGate: true,
          sessionCapture: false,
          maintenanceReminders: true,
        },
      });

      expect(run(home, ['config', 'validate'])).toMatchObject({ status: 0, payload: { ok: true } });
      expect(run(home, ['config', 'init', '--hooks', 'true'])).toMatchObject({ status: 1 });
    } finally { rmSync(home, { recursive: true, force: true }); }
  });
});
