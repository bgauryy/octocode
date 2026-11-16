import { afterAll, describe, expect, it } from 'vitest';
import { spawn, spawnSync } from 'node:child_process';
import {
  chmodSync,
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { once } from 'node:events';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import {
  installNativeBin,
  installNotice,
} from '../../src/cli/native-install.js';
import { allowLocalExecutables } from '../../../../test-utils/external-effects-guard.js';

const VERSION = '1.2.3';
const STUB = resolve(__dirname, '../../src/bin.mjs');
const posix = process.platform !== 'win32';
const tempDirs: string[] = [];

afterAll(() => {
  for (const dir of tempDirs) rmSync(dir, { recursive: true, force: true });
});

/**
 * A fake native binary: `--version` names `version`; `wait` says `ready` and
 * sleeps until a signal; anything else copies stdin to stdout, the arguments to stderr, and
 * exits 7.
 */
function fakeNative(dir: string, version = VERSION): string {
  const bin = join(dir, 'octocode');
  writeFileSync(
    bin,
    [
      '#!/bin/sh',
      `if [ "$1" = --version ]; then echo "octocode ${version}"; exit 0; fi`,
      'if [ "$1" = wait ]; then echo ready; exec sleep 30; fi',
      'cat',
      'printf "%s|" "$@" >&2',
      'exit 7',
      '',
    ].join('\n')
  );
  chmodSync(bin, 0o755);
  allowLocalExecutables(bin);
  return bin;
}

/**
 * An installed `octocode` package as npm lays it out: the manifest, the
 * built bin stub (out/octocode) and a launcher (out/octocode.js) that says
 * it ran, next to its platform package's native binary.
 */
function installedLayout(
  nativeVersion = VERSION,
  under: string[] = [],
  modules = 'node_modules'
) {
  const root = mkdtempSync(join(tmpdir(), 'octocode-install-'));
  tempDirs.push(root);
  const packageRoot = join(root, ...under, modules, 'octocode');
  const nativeDir = join(
    root,
    ...under,
    'node_modules',
    '@octocodeai',
    'native'
  );
  mkdirSync(join(packageRoot, 'out'), { recursive: true });
  mkdirSync(nativeDir, { recursive: true });
  writeFileSync(
    join(packageRoot, 'package.json'),
    JSON.stringify({
      name: 'octocode',
      version: VERSION,
      type: 'module',
      bin: './out/octocode',
    })
  );
  const bin = join(packageRoot, 'out', 'octocode');
  copyFileSync(STUB, bin);
  chmodSync(bin, 0o755);
  allowLocalExecutables(bin);
  writeFileSync(
    join(packageRoot, 'out', 'octocode.js'),
    'process.stdout.write(`launcher ${process.argv.slice(2).join(" ")}\\n`);\n'
  );
  return {
    packageRoot,
    bin,
    native: fakeNative(nativeDir, nativeVersion),
    stub: readFileSync(STUB),
  };
}

const outEntries = (packageRoot: string) =>
  readdirSync(join(packageRoot, 'out')).sort();

describe.skipIf(!posix)('installNativeBin', () => {
  it('makes the bin the native binary, reporting the pinned version', () => {
    const layout = installedLayout();
    const outcome = installNativeBin({
      packageRoot: layout.packageRoot,
      version: VERSION,
      env: {},
      resolveNative: () => layout.native,
    });
    expect(outcome).toEqual({ bin: 'native', path: layout.bin });
    // The same file, not a script that starts it: no Node parent.
    expect(statSync(layout.bin).ino).toBe(statSync(layout.native).ino);
    const version = spawnSync(layout.bin, ['--version'], { encoding: 'utf8' });
    expect(version.stdout).toBe(`octocode ${VERSION}\n`);
    expect(outEntries(layout.packageRoot)).toEqual(['octocode', 'octocode.js']);
  });

  it('passes stdin, stdout, stderr, arguments and the exit code through', () => {
    const layout = installedLayout();
    installNativeBin({
      packageRoot: layout.packageRoot,
      version: VERSION,
      env: {},
      resolveNative: () => layout.native,
    });
    const run = spawnSync(layout.bin, ['a b', '--x=y=z', 'λ'], {
      input: 'stdin text',
      encoding: 'utf8',
    });
    expect(run).toMatchObject({
      status: 7,
      stdout: 'stdin text',
      stderr: 'a b|--x=y=z|λ|',
    });
  });

  it('delivers signals to the native process itself', async () => {
    const layout = installedLayout();
    installNativeBin({
      packageRoot: layout.packageRoot,
      version: VERSION,
      env: {},
      resolveNative: () => layout.native,
    });
    const child = spawn(layout.bin, ['wait'], {
      stdio: ['ignore', 'pipe', 'ignore'],
    });
    const exit = new Promise(done => child.once('exit', (_, s) => done(s)));
    const [first] = (await once(child.stdout, 'data')) as [Buffer];
    expect(first.toString()).toBe('ready\n');
    child.kill('SIGTERM');
    await expect(exit).resolves.toBe('SIGTERM');
  });

  it('replaces a native bin left by an earlier install (upgrade)', () => {
    const layout = installedLayout();
    const old = fakeNative(
      mkdtempSync(join(tmpdir(), 'octocode-old-')),
      '1.0.0'
    );
    tempDirs.push(join(old, '..'));
    copyFileSync(old, layout.bin);
    installNativeBin({
      packageRoot: layout.packageRoot,
      version: VERSION,
      env: {},
      resolveNative: () => layout.native,
    });
    expect(
      spawnSync(layout.bin, ['--version'], { encoding: 'utf8' }).stdout
    ).toBe(`octocode ${VERSION}\n`);
  });

  it('copies the binary when a hard link is refused', () => {
    const layout = installedLayout();
    const outcome = installNativeBin({
      packageRoot: layout.packageRoot,
      version: VERSION,
      env: {},
      resolveNative: () => layout.native,
      link: () => {
        throw Object.assign(new Error('EXDEV'), { code: 'EXDEV' });
      },
    });
    expect(outcome).toEqual({ bin: 'native', path: layout.bin });
    expect(readFileSync(layout.bin)).toEqual(readFileSync(layout.native));
    expect(statSync(layout.bin).mode & 0o111).not.toBe(0);
  });

  describe('keeps the Node launcher bin, untouched', () => {
    const kept = (
      layout: ReturnType<typeof installedLayout>,
      outcome: ReturnType<typeof installNativeBin>,
      reason: RegExp,
      expected = false
    ) => {
      expect(outcome).toEqual({
        bin: 'node',
        reason: expect.any(String),
        expected,
      });
      expect('reason' in outcome && outcome.reason).toMatch(reason);
      expect(readFileSync(layout.bin)).toEqual(layout.stub);
      expect(outEntries(layout.packageRoot)).toEqual([
        'octocode',
        'octocode.js',
      ]);
      // The kept bin still starts the launcher with the arguments.
      expect(
        spawnSync(layout.bin, ['--version'], { encoding: 'utf8' }).stdout
      ).toBe('launcher --version\n');
    };

    it('when the platform package is missing', () => {
      const layout = installedLayout();
      const outcome = installNativeBin({
        packageRoot: layout.packageRoot,
        version: VERSION,
        env: {},
        resolveNative: () => {
          throw new Error("platform package 'x' is not installed.");
        },
      });
      kept(layout, outcome, /is not installed/);
    });

    it('when the binary reports another version', () => {
      const layout = installedLayout('9.9.9');
      const outcome = installNativeBin({
        packageRoot: layout.packageRoot,
        version: VERSION,
        env: {},
        resolveNative: () => layout.native,
      });
      kept(layout, outcome, /reports 'octocode 9\.9\.9'/);
    });

    it('when the binary cannot run here', () => {
      const layout = installedLayout();
      chmodSync(layout.native, 0o644);
      const outcome = installNativeBin({
        packageRoot: layout.packageRoot,
        version: VERSION,
        env: {},
        resolveNative: () => layout.native,
      });
      kept(layout, outcome, /EACCES/);
    });

    it('on Windows, whose bin shims start Node', () => {
      const layout = installedLayout();
      const outcome = installNativeBin({
        packageRoot: layout.packageRoot,
        version: VERSION,
        env: {},
        platform: 'win32',
        resolveNative: () => layout.native,
      });
      kept(layout, outcome, /Windows/, true);
    });

    it('under Yarn 2 or later, which runs bins through Node', () => {
      const layout = installedLayout();
      const outcome = installNativeBin({
        packageRoot: layout.packageRoot,
        version: VERSION,
        env: { npm_config_user_agent: 'yarn/4.9.1 npm/? node/v24.15.0' },
        resolveNative: () => layout.native,
      });
      kept(layout, outcome, /Yarn 4/, true);
    });

    it('in a pnpm global install, whose bin shim is not rewritten after a script approval', () => {
      const layout = installedLayout(VERSION, ['home', 'global', 'v11', 'abc']);
      const outcome = installNativeBin({
        packageRoot: layout.packageRoot,
        version: VERSION,
        env: { npm_config_user_agent: 'pnpm/12.10.1 npm/? node/v24 darwin' },
        resolveNative: () => layout.native,
      });
      kept(layout, outcome, /pnpm global/, true);
    });

    it('in a source checkout, which no node_modules directory contains', () => {
      const layout = installedLayout(VERSION, [], 'packages');
      const outcome = installNativeBin({
        packageRoot: layout.packageRoot,
        version: VERSION,
        env: {},
        resolveNative: () => layout.native,
      });
      kept(layout, outcome, /source checkout/, true);
    });

    it('without a pinned version (an unbundled source run)', () => {
      const layout = installedLayout();
      const outcome = installNativeBin({
        packageRoot: layout.packageRoot,
        version: undefined,
        env: {},
        resolveNative: () => layout.native,
      });
      kept(layout, outcome, /pinned/);
    });
  });

  it('optimizes under Yarn 1, npm, pnpm and Bun', () => {
    for (const agent of [
      'yarn/1.22.22 npm/? node/v24.15.0 darwin arm64',
      'npm/11.17.0 node/v24.15.0 darwin arm64',
      'pnpm/10.20.0 npm/? node/v24.15.0 darwin arm64',
      'bun/1.4.3 npm/? node/v24.15.0 darwin arm64',
    ]) {
      const layout = installedLayout();
      expect(
        installNativeBin({
          packageRoot: layout.packageRoot,
          version: VERSION,
          env: { npm_config_user_agent: agent },
          resolveNative: () => layout.native,
        }),
        agent
      ).toEqual({ bin: 'native', path: layout.bin });
    }
  });
});

describe('installNotice', () => {
  it('stays silent when the bin is native or the platform expects the Node launcher', () => {
    expect(installNotice({ bin: 'native', path: '/x' })).toBeUndefined();
    expect(
      installNotice({ bin: 'node', reason: 'Windows', expected: true })
    ).toBeUndefined();
  });

  it('says why when the Node launcher stays for an unexpected reason', () => {
    expect(
      installNotice({
        bin: 'node',
        reason: 'platform package missing',
        expected: false,
      })
    ).toBe(
      'octocode: the octocode command stays the Node launcher: platform package missing'
    );
  });
});

describe('postinstall entry', () => {
  it('does nothing in a checkout whose out/ is not built yet', () => {
    const root = mkdtempSync(join(tmpdir(), 'octocode-postinstall-'));
    tempDirs.push(root);
    mkdirSync(join(root, 'scripts'));
    copyFileSync(
      resolve(__dirname, '../../scripts/postinstall.mjs'),
      join(root, 'scripts', 'postinstall.mjs')
    );
    const run = spawnSync(
      process.execPath,
      [join(root, 'scripts', 'postinstall.mjs')],
      { encoding: 'utf8' }
    );
    expect(run).toMatchObject({ status: 0, stdout: '', stderr: '' });
  });
});
