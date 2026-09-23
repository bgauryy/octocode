import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

// End-to-end: the BUILT launcher against the REAL packaged native binary —
// the exact path `npx octocode` takes. Everything else in this suite mocks
// the delegation boundary; this file is the one place the boundary is real.
// Skips when either artifact is absent (unbuilt checkout, platforms without
// committed binaries).

const require = createRequire(import.meta.url);
const launcher = resolve(__dirname, '..', '..', 'out', 'octocode.js');

function nativeBinary(): string | null {
  try {
    const packageRoot = dirname(
      require.resolve('@octocodeai/octocode-native/package.json')
    );
    // platform.cjs is not on the package exports map; load it by path.
    const platform = require(join(packageRoot, 'bin', 'platform.cjs')) as {
      getPlatformSuffix: () => string | null;
    };
    const suffix = platform.getPlatformSuffix();
    if (!suffix) return null;
    const binary = join(
      packageRoot,
      'npm',
      suffix,
      process.platform === 'win32' ? 'octocode.exe' : 'octocode'
    );
    return existsSync(binary) ? binary : null;
  } catch {
    return null;
  }
}

const binary = nativeBinary();
const ready = existsSync(launcher) && binary !== null;

function runLauncher(args: string[], env: NodeJS.ProcessEnv = {}) {
  return spawnSync(process.execPath, [launcher, ...args], {
    encoding: 'utf8',
    env: { ...process.env, NO_COLOR: '1', ...env },
    timeout: 60_000,
  });
}

describe.skipIf(!ready)('launcher → native binary e2e', () => {
  it("serves --version with launcher and native versions", () => {
    const result = runLauncher(['--version']);
    expect(result.status).toBe(0);
    expect(result.stdout).toMatch(/^octocode \d+\.\d+\.\d+ \(native \d+\.\d+\.\d+\)/);
  });

  it('executes a real tool call end-to-end with exit 0 and structured JSON', () => {
    const query = JSON.stringify({
      reasoning: 'Launcher e2e: prove the delegation boundary executes tools.',
      path: resolve(__dirname, '..', '..', 'src'),
      searchText: 'delegateToNative',
      pageSize: 5,
    });
    const result = runLauncher(['localSearch', query, '--compact']);
    expect(result.status).toBe(0);
    const payload = JSON.parse(result.stdout) as {
      results: Array<{ index: number }>;
    };
    expect(payload.results[0].index).toBe(0);
  });

  it('propagates native validation failures as exit 2', () => {
    const result = runLauncher(['localSearch', '{"path":"/tmp"}', '--compact']);
    expect(result.status).toBe(2);
  });

  it('keeps the skill recursion guard intact in the packaged binary', () => {
    // The guard only applies to subcommands the native binary DELEGATES to the
    // npm CLI. list/install/remove/check/info are served natively (no delegation,
    // no guard), so exercise a non-native subcommand that must delegate.
    const result = spawnSync(binary as string, ['skill', 'sync'], {
      encoding: 'utf8',
      env: { ...process.env, OCTOCODE_SKILL_DELEGATED: '1' },
      timeout: 30_000,
    });
    expect(result.status).toBe(1);
    expect(result.stderr).toContain('native binary, not the npm CLI');
  });
});
