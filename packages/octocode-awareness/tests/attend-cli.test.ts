import { describe, expect, it } from 'vitest';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const cli = fileURLToPath(new URL('../out/octocode-awareness.js', import.meta.url));

describe('built CLI operational regulation', () => {
  it('senses verification pressure and clears it only after an observed check receipt', () => {
    const workspace = mkdtempSync(join(realpathSync(tmpdir()), 'physiology-cli-'));
    const db = join(workspace, 'awareness.sqlite3');
    const run = (args: string[]) => {
      const result = spawnSync(process.execPath, [cli, ...args, '--db', db, '--workspace', workspace], {
        encoding: 'utf8', timeout: 30_000, cwd: workspace,
      });
      expect(result.status, result.stderr || result.stdout).toBe(0);
      return JSON.parse(result.stdout) as Record<string, any>;
    };
    try {
      const args = ['attend', '--agent-id', 'owner', '--compact'];
      const empty = run(args);
      expect(empty.regulation).toEqual({ advisory: true, actions: [] });
      expect(empty.operational_state.unavailable).toContain('context');
      expect(empty.next.action).toBe('continue');
      expect(empty.next.command).toBeUndefined();
      const work = run(['work', 'start', '--agent-id', 'owner', '--file', 'example.ts',
        '--rationale', 'verify pressure recovery', '--test-plan', 'node -e process.exit(0)', '--compact']);
      run(['work', 'end', '--agent-id', 'owner', '--run-id', String(work.run_id), '--compact']);
      const pending = run(args);
      expect(pending.regulation.actions).toContain('verify_owned_work');
      expect(pending.operational_state.verification.owned_observed).toBe(1);
      expect(pending.next.command.name).toBe('verify audit');
      const selected = spawnSync(process.execPath, [cli, ...pending.next.command.name.split(' '), ...pending.next.command.args], {
        encoding: 'utf8', timeout: 30_000, cwd: tmpdir(),
        env: { ...process.env, OCTOCODE_HOME: join(workspace, 'unrelated-home') },
      });
      // Audit returns 1 while debt exists; its read succeeded in the selected store.
      expect(selected.status, selected.stderr || selected.stdout).toBe(1);
      expect(JSON.parse(selected.stdout)).toMatchObject({ ok: true, unverified_count: 1 });
      expect(selected.stdout).toContain(String(work.run_id));
      const full = run(['attend', '--agent-id', 'owner']);
      expect(full.operational_state).toEqual(pending.operational_state);
      expect(full.regulation).toEqual(pending.regulation);
      const check = spawnSync(process.execPath, ['-e', 'process.exit(0)']);
      expect(check.status).toBe(0);
      run(['verify', 'mark', '--agent-id', 'owner', '--run-id', String(work.run_id),
        '--message', 'Observed node -e process.exit(0): exit 0', '--compact']);
      const recovered = run(args);
      expect(recovered.operational_state.verification.owned_observed).toBe(0);
      expect(recovered.regulation.actions).not.toContain('verify_owned_work');
      expect(run(['attend', '--agent-id', 'owner', '--compact', '--explain-organ']).organ_reference).toBeDefined();
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });
});
