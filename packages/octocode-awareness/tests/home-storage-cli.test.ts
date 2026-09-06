import { describe, expect, it } from 'vitest';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const cli = fileURLToPath(new URL('../out/octocode-awareness.js', import.meta.url));

describe('home storage across CLI features', () => {
  it('lets attend see recorded memory in the same workspace while preserving workspace and store isolation', () => {
    const root = mkdtempSync(join(realpathSync(tmpdir()), 'awareness-home-cli-'));
    const home = join(root, 'home');
    const workspace = join(root, 'workspace');
    const other = join(root, 'other');
    for (const dir of [home, workspace, other]) mkdirSync(dir);
    const run = (args: string[], cwd = workspace): Record<string, any> => {
      const result = spawnSync(process.execPath, [cli, ...args, '--workspace', cwd, '--compact'], {
        cwd, encoding: 'utf8', timeout: 30_000, env: { ...process.env, OCTOCODE_HOME: home },
      });
      expect(result.status, result.stderr || result.stdout).toBe(0);
      return JSON.parse(result.stdout) as Record<string, any>;
    };
    try {
      run(['memory', 'record', '--agent-id', 'owner', '--task-context', 'sentinel cache isolation',
        '--observation', 'sentinel cache isolation uses workspace identity', '--importance', '8', '--label', 'GOTCHA']);
      const packet = run(['attend', '--agent-id', 'owner', '--query', 'sentinel cache isolation']);
      expect(packet.evidence).toHaveLength(1);
      expect(run(['attend', '--agent-id', 'owner', '--query', 'sentinel cache isolation'], other).evidence).toHaveLength(0);
      expect(existsSync(join(home, 'awareness', 'awareness.sqlite3'))).toBe(true);
      expect(existsSync(join(workspace, '.octocode', 'awareness.sqlite3'))).toBe(false);
      const isolated = run(['attend', '--agent-id', 'owner', '--query', 'sentinel cache isolation', '--db-scope', 'repo']);
      expect(isolated.evidence).toHaveLength(0);
      expect(existsSync(join(workspace, '.octocode', 'awareness.sqlite3'))).toBe(true);
      expect(run(['attend', '--agent-id', 'owner', '--query', 'sentinel cache isolation']).evidence).toHaveLength(1);
    } finally { rmSync(root, { recursive: true, force: true }); }
  });
});
