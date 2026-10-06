import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { existsSync, readFileSync } from 'node:fs';
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
  it.each(['', 'discovery-only-test-key'])(
    'serves the schema catalog for a bare piped call, with classification key %j',
    key => {
      const env = { OCTOCODE_CLASSIFICATION_API: key };
      const main = runLauncher([], env);
      const schema = runLauncher(['schema'], env);
      expect(main.status).toBe(0);
      expect(schema.status).toBe(0);
      const instructions = JSON.parse(main.stdout).instructions as string;
      expect(instructions.length).toBeGreaterThan(0);
      expect(JSON.parse(schema.stdout).instructions).toBe(instructions);
      expect(/\bclasify\b/i.test(instructions)).toBe(Boolean(key));
    }
  );

  it('prints native root help without agent instructions', () => {
    for (const args of [['--help'], ['-h'], ['help']]) {
      const help = runLauncher(args);
      expect(help.status).toBe(0);
      expect(help.stdout).toMatch(/^\s+schema\s/m);
      expect(help.stdout).not.toContain('Agent instructions');
      expect(help.stdout).not.toMatch(/\bscheme\b|showConfig|--json-errors/);
    }
    const schemaHelp = runLauncher(['schema', '--help']);
    expect(schemaHelp.status).toBe(0);
    expect(schemaHelp.stdout).toContain('Usage: octocode schema');
  });

  it('reports errors as the JSON envelope on a pipe', () => {
    const typo = runLauncher(['lokalSearch', '{}']);
    expect(typo.status).toBe(2);
    expect(JSON.parse(typo.stdout)).toMatchObject({
      kind: 'octocode.toolError',
    });
    const badJson = runLauncher(['localSearch', '{"queries":[']);
    expect(badJson.status).toBe(2);
    expect(JSON.parse(badJson.stdout).error).toContain('Invalid JSON query');
  });

  it('reads the query from stdin with --input -', () => {
    const result = spawnSync(
      process.execPath,
      [launcher, 'localFetch', '--input', '-'],
      {
        encoding: 'utf8',
        env: { ...process.env, NO_COLOR: '1' },
        input: JSON.stringify({
          queries: [{ path: launcher, ranges: ['1-1'] }],
        }),
        timeout: 60_000,
      }
    );
    expect(result.status, result.stderr).toBe(0);
    expect(JSON.parse(result.stdout).results[0].index).toBe(0);
  });

  it('serves --version as the one package version', () => {
    const result = runLauncher(['--version']);
    expect(result.status).toBe(0);
    expect(result.stdout).toBe(
      `octocode ${JSON.parse(readFileSync(resolve(__dirname, '..', '..', 'package.json'), 'utf8')).version}\n`
    );
  });

  it('executes a real tool call end-to-end with exit 0 and structured JSON', () => {
    const query = JSON.stringify({
      queries: [
        {
          path: resolve(__dirname, '..', '..', 'src'),
          matchString: 'delegateToNative',
          pageSize: 5,
        },
      ],
    });
    const result = runLauncher(['localSearch', query]);
    expect(result.status).toBe(0);
    const payload = JSON.parse(result.stdout) as {
      results: Array<{ index: number }>;
    };
    expect(payload.results[0].index).toBe(0);
  });

  it('propagates native validation failures as exit 2', () => {
    const result = runLauncher(['localSearch', '{"path":"/tmp"}']);
    expect(result.status).toBe(2);
  });

  it('keeps the skill recursion guard intact in the packaged binary', () => {
    // The native binary forwards every `skill` subcommand to the npm CLI. With
    // the delegation marker already set, the guard refuses to delegate again
    // and names the npm launcher instead of recursing.
    const result = spawnSync(binary as string, ['skill', 'sync'], {
      encoding: 'utf8',
      env: { ...process.env, OCTOCODE_SKILL_DELEGATED: '1' },
      timeout: 30_000,
    });
    expect(result.status).toBe(1);
    expect(result.stderr).toMatch(/npm launcher owns `skill`/);
    expect(result.stderr).toContain('octocode skill sync');
  });
});
