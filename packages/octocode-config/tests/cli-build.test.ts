import { execFileSync, spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { beforeAll, describe, expect, it } from 'vitest';

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const builtCli = resolve(packageRoot, 'dist/cli.js');

describe('built octocode-config CLI', () => {
  beforeAll(() => {
    execFileSync(process.execPath, ['build.mjs'], {
      cwd: packageRoot,
      stdio: 'pipe',
    });
  });

  it('executes the generated binary without a duplicate shebang', () => {
    const result = spawnSync(process.execPath, [builtCli, '--help'], {
      cwd: packageRoot,
      encoding: 'utf8',
    });

    expect(result.status).toBe(0);
    expect(result.stderr).toBe('');
    expect(result.stdout).toContain('@octocodeai/config');
  });
});
