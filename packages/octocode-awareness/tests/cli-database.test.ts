import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const sourceCli = resolve(packageRoot, 'bin/awareness.ts');

function run(...args: string[]) {
  return spawnSync(process.execPath, ['--import', 'tsx', sourceCli, ...args], {
    cwd: packageRoot,
    encoding: 'utf8',
    timeout: 30_000,
  });
}

describe('database consolidation source CLI', () => {
  it('documents the explicit read-only copy command', () => {
    const result = run('database', 'consolidate', '--help');
    expect(result.status, result.stderr).toBe(0);
    expect(result.stdout).toContain('database consolidate --source <existing-file> --destination <new-file>');
    expect(result.stdout).toContain('--unattributed-agent-id <actor>');
  });

  it('returns a structured error before opening an incomplete command', () => {
    const result = run('database', 'consolidate', '--source', '/tmp/source.sqlite3', '--compact');
    expect(result.status, result.stderr).toBe(1);
    expect(JSON.parse(result.stdout)).toMatchObject({ ok: false, error_code: 'DATABASE_CONSOLIDATION_ERROR', error: '--destination requires a value' });
  });
});
