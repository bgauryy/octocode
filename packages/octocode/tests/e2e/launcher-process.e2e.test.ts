import { spawn } from 'node:child_process';
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { createServer, type Server } from 'node:http';
import { createHash } from 'node:crypto';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { gzipSync } from 'node:zlib';
import { afterAll, describe, expect, it } from 'vitest';
import { launcher } from './native-fixture.js';

// The BUILT launcher as a process: exit status, signal forwarding, a missing
// platform package and the registry fallback, each against fake native
// binaries so no real binary or network is needed. POSIX only: the fakes are
// scripts with a shebang.

type Run = { code: number | null; stdout: string; stderr: string };

const pins: unknown = Object.values(
  JSON.parse(readFileSync(join(__dirname, '..', '..', 'package.json'), 'utf8'))
    .optionalDependencies
)[0];
/** The platform package version the bundle pins (and downloads). */
const nativeVersion = typeof pins === 'string' ? pins : '';

const tempDirs: string[] = [];

afterAll(() => {
  for (const dir of tempDirs) rmSync(dir, { recursive: true, force: true });
});

function tempDir(): string {
  const dir = mkdtempSync(join(tmpdir(), 'octocode-launcher-'));
  tempDirs.push(dir);
  return dir;
}

function script(dir: string, name: string, body: string): string {
  const path = join(dir, name);
  writeFileSync(path, body);
  chmodSync(path, 0o755);
  return path;
}

/** A Node script standing in for the native binary. */
function fakeNative(body: string): string {
  return script(tempDir(), 'octocode', `#!${process.execPath}\n${body}\n`);
}

/** Runs `node <entry> ...args` asynchronously (a test server may be serving). */
function runLauncher(
  args: string[],
  env: NodeJS.ProcessEnv,
  onStdout?: (text: string, pid: number) => void,
  entry = launcher
): Promise<Run> {
  const child = spawn(process.execPath, [entry, ...args], {
    env: { ...process.env, ...env },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let stdout = '';
  let stderr = '';
  child.stdout.setEncoding('utf8').on('data', (chunk: string) => {
    stdout += chunk;
    if (child.pid !== undefined) onStdout?.(stdout, child.pid);
  });
  child.stderr.setEncoding('utf8').on('data', (chunk: string) => {
    stderr += chunk;
  });
  return new Promise(resolve => {
    child.once('close', code => resolve({ code, stdout, stderr }));
  });
}

const devOverride = (bin: string) => ({
  NODE_ENV: 'test',
  OCTOCODE_NATIVE_BIN: bin,
});

/** The launcher alone in a directory with no node_modules above it. */
function isolatedLauncher(): string {
  const dir = tempDir();
  writeFileSync(join(dir, 'package.json'), '{"type":"module"}');
  const entry = join(dir, 'octocode.js');
  copyFileSync(launcher, entry);
  return entry;
}

/** A gzipped npm tarball holding one executable `package/<name>`. */
function npmTarball(name: string, body: string): Buffer {
  const header = Buffer.alloc(512);
  const data = Buffer.from(body);
  header.write(`package/${name}`, 0);
  header.write('0000755\0', 100);
  header.write('0000000\0', 108);
  header.write('0000000\0', 116);
  header.write(`${data.length.toString(8).padStart(11, '0')}\0`, 124);
  header.write('00000000000\0', 136);
  header.write('0', 156);
  header.write('ustar\u000000', 257);
  header.fill(' ', 148, 156);
  const sum = header.reduce((total, byte) => total + byte, 0);
  header.write(`${sum.toString(8).padStart(6, '0')}\0 `, 148);
  const padding = Buffer.alloc((512 - (data.length % 512)) % 512);
  return gzipSync(Buffer.concat([header, data, padding, Buffer.alloc(1024)]));
}

function portOf(server: Server): number {
  const address = server.address();
  if (address === null || typeof address === 'string') {
    throw new Error('the registry fixture is not listening on TCP');
  }
  return address.port;
}

/** A loopback npm registry serving one platform package tarball. */
async function registryServing(tarball: Buffer) {
  const integrity = `sha512-${createHash('sha512').update(tarball).digest('base64')}`;
  const requests: string[] = [];
  const server = createServer((request, response) => {
    const url = request.url ?? '';
    requests.push(url);
    if (url === '/pkg.tgz') {
      response.end(tarball);
      return;
    }
    response.setHeader('content-type', 'application/json');
    response.end(
      JSON.stringify({
        dist: {
          tarball: `http://127.0.0.1:${portOf(server)}/pkg.tgz`,
          integrity,
        },
      })
    );
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  return { server, requests, url: `http://127.0.0.1:${portOf(server)}/` };
}

/** Runs the launcher on `bin` and sends SIGTERM to it once native is up. */
function terminateWhenReady(bin: string): Promise<Run> {
  let sent = false;
  return runLauncher([], devOverride(bin), (stdout, pid) => {
    if (sent || !stdout.includes('ready')) return;
    sent = true;
    process.kill(pid, 'SIGTERM');
  });
}

const posixBuilt = process.platform !== 'win32' && existsSync(launcher);

describe.skipIf(!posixBuilt)('built launcher process boundary', () => {
  it('passes arguments through and returns the native exit code', async () => {
    const bin = fakeNative(
      'process.stdout.write(JSON.stringify(process.argv.slice(2))); process.exit(7);'
    );
    const args = ['localSearch', 'a b', '--flag=x=y', 'λ'];
    const run = await runLauncher(args, devOverride(bin));
    expect(run).toEqual({
      code: 7,
      stdout: JSON.stringify(args),
      stderr: '',
    });
  });

  it('forwards SIGTERM to native and returns its exit code', async () => {
    const marker = join(tempDir(), 'signal');
    const bin = fakeNative(`
        process.on('SIGTERM', () => {
          require('node:fs').writeFileSync(${JSON.stringify(marker)}, 'SIGTERM');
          process.exit(42);
        });
        setInterval(() => {}, 1000);
        process.stdout.write('ready');
      `);
    const run = await terminateWhenReady(bin);
    expect(run.code).toBe(42);
    expect(readFileSync(marker, 'utf8')).toBe('SIGTERM');
  });

  it('reports a native signal death as 128+N', async () => {
    const bin = fakeNative(
      "setInterval(() => {}, 1000); process.stdout.write('ready');"
    );
    const run = await terminateWhenReady(bin);
    expect(run.code).toBe(143);
  });
});

describe.skipIf(!posixBuilt)(
  'built launcher without its platform package',
  () => {
    it('names the missing platform package and the failed download', async () => {
      const run = await runLauncher(
        ['--version'],
        { npm_config_registry: 'http://127.0.0.1:9/' },
        undefined,
        isolatedLauncher()
      );
      expect(run.code).toBe(5);
      expect(run.stdout).toBe('');
      expect(run.stderr).toMatch(
        /platform package '@octocodeai\/octocode-native-[\w-]+' is not installed/
      );
      expect(run.stderr).toContain('Reinstall with optional dependencies');
      expect(run.stderr).toContain('Downloading it from the registry failed');
    });

    it('downloads the pinned platform package when it is missing', async () => {
      const tarball = npmTarball(
        'octocode',
        `#!/bin/sh\nif [ "$1" = --version ]; then echo "octocode ${nativeVersion}"; exit 0; fi\necho "fetched native: $*"; exit 3\n`
      );
      const registry = await registryServing(tarball);
      try {
        const env = {
          npm_config_registry: registry.url,
          OCTOCODE_HOME: tempDir(),
        };
        const entry = isolatedLauncher();
        const first = await runLauncher(['schema', 'x'], env, undefined, entry);
        expect(first.code).toBe(3);
        expect(first.stdout).toBe('fetched native: schema x\n');
        expect(first.stderr).toContain(`downloading `);
        expect(registry.requests).toEqual([
          expect.stringContaining(`%2foctocode-native-`),
          '/pkg.tgz',
        ]);
        // Cached: the second launch makes no request.
        const second = await runLauncher(['a'], env, undefined, entry);
        expect(second).toEqual({
          code: 3,
          stdout: 'fetched native: a\n',
          stderr: '',
        });
        expect(registry.requests).toHaveLength(2);
      } finally {
        registry.server.close();
      }
    });
  }
);
