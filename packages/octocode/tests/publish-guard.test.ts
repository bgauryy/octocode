import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { expect, it } from 'vitest';

it('recognizes literal and glob workspace members outside packages, but rejects external local dependencies', () => {
  const root = mkdtempSync(join(tmpdir(), 'octocode-publish-guard-'));
  try {
    const write = (path: string, value: unknown) => {
      mkdirSync(resolve(root, path, '..'), { recursive: true });
      writeFileSync(join(root, path), JSON.stringify(value));
    };
    write('package.json', {
      name: 'fixture',
      workspaces: ['packages/*', 'skills', 'skills/communication'],
    });
    for (const name of [
      'octocode',
      'octocode-config',
      'octocode-native',
      'octocode-mcp',
    ]) {
      write(`packages/${name}/package.json`, { name, version: '1.0.0' });
    }
    write('skills/package.json', { name: 'skill-root', version: '1.0.0' });
    write('skills/communication/package.json', {
      name: 'communication',
      version: '1.0.0',
      private: true,
    });
    const script = join(
      root,
      'packages/octocode/scripts/check-no-workspace-protocol.mjs'
    );
    mkdirSync(resolve(script, '..'), { recursive: true });
    writeFileSync(
      script,
      readFileSync(
        resolve(__dirname, '../scripts/check-no-workspace-protocol.mjs')
      )
    );
    const lock =
      '  resolution: "communication@workspace:skills/communication"\n  resolution: "skill-root@workspace:skills"\n';
    writeFileSync(join(root, 'yarn.lock'), lock);
    expect(
      spawnSync(process.execPath, [script], { encoding: 'utf8' }).status
    ).toBe(0);
    writeFileSync(
      join(root, 'yarn.lock'),
      lock + '  resolution: "external-core@file:../core"\n'
    );
    const failure = spawnSync(process.execPath, [script], { encoding: 'utf8' });
    expect(failure.status).toBe(1);
    expect(failure.stderr).toContain('external-core: resolved via file:');
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
