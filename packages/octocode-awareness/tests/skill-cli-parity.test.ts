import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { executeAwarenessCli } from '../src/command-cli.js';

describe('public skill CLI parity', () => {
  let root: string;

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'awareness-skill-cli-'));
    vi.stubEnv('OCTOCODE_HOME', join(root, 'octocode-home'));
  });

  afterEach(() => {
    vi.unstubAllEnvs();
    rmSync(root, { recursive: true, force: true });
  });

  it('installs, lists, checks, previews removal, and removes through the public CLI', async () => {
    const install = await executeAwarenessCli([
      'skill',
      'install',
      '--platform',
      'codex',
      '--project-dir',
      root,
      '--upgrade',
      '--compact',
    ]);
    expect(install.exitCode, JSON.stringify(install.payload)).toBe(0);
    expect(install.payload).toMatchObject({
      ok: true,
      upgrade: true,
      action: 'upgrade',
    });

    const list = await executeAwarenessCli(['skill', 'list', '--compact']);
    expect(list.exitCode, JSON.stringify(list.payload)).toBe(0);
    expect(list.payload).toMatchObject({
      ok: true,
      action: 'list',
      count: 1,
      installedCount: 1,
    });

    const check = await executeAwarenessCli([
      'skill',
      'check',
      '--platform',
      'shared',
      '--project-dir',
      root,
      '--compact',
    ]);
    expect(check.exitCode, JSON.stringify(check.payload)).toBe(0);
    expect(check.payload).toMatchObject({ ok: true, action: 'check' });

    const link = join(root, '.agents/skills/octocode-awareness');
    expect(existsSync(link)).toBe(true);
    const preview = await executeAwarenessCli([
      'skill',
      'remove',
      '--platform',
      'codex-native',
      '--project-dir',
      root,
      '--compact',
    ]);
    expect(preview.exitCode, JSON.stringify(preview.payload)).toBe(0);
    expect(preview.payload).toMatchObject({
      ok: true,
      action: 'dry-run',
      summary: { wouldRemove: 1 },
    });
    expect(existsSync(link)).toBe(true);

    const removed = await executeAwarenessCli([
      'skill',
      'remove',
      '--platform',
      'codex',
      '--project-dir',
      root,
      '--confirm',
      '--compact',
    ]);
    expect(removed.exitCode, JSON.stringify(removed.payload)).toBe(0);
    expect(removed.payload).toMatchObject({
      ok: true,
      action: 'remove',
      summary: { removed: 1 },
    });
    expect(existsSync(link)).toBe(false);
  });

  it('serves focused help for every skill command', async () => {
    for (const command of ['install', 'list', 'check', 'remove']) {
      const result = await executeAwarenessCli(['skill', command, '--help']);
      expect(result.exitCode, command).toBe(0);
      expect(result.text, command).toContain(`skill ${command}`);
    }
  });
});
