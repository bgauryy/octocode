import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, it } from 'vitest';

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');

// Its own file: the spawned generator takes seconds, and a file runs beside the others.
it('the tool contract generator runs its entry point when called through a symlink', () => {
  // --check is read-only; an empty PATH hides cargo-typify, so a run that reaches main() fails loudly.
  const dir = mkdtempSync(join(tmpdir(), 'contract-link-'));
  try {
    const link = join(dir, 'generate-tool-contract.ts');
    symlinkSync(resolve(packageRoot, 'scripts/generate-tool-contract.ts'), link);
    const run = spawnSync(process.execPath, [link, '--check'], {
      encoding: 'utf8',
      env: { PATH: '' },
    });
    expect(run.error).toBeUndefined();
    expect(run.stderr).toMatch(/cargo-typify .*is required/);
    expect(run.status).not.toBe(0);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
