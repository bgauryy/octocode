// Run after native, CLI, and MCP debug builds. All secrets and homes are fixtures.
import assert from 'node:assert/strict';
import { createCipheriv, createDecipheriv, randomBytes } from 'node:crypto';
import { spawn } from 'node:child_process';
import {
  mkdtemp,
  mkdir,
  writeFile,
  readFile,
  rm,
  stat,
} from 'node:fs/promises';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
const require = createRequire(import.meta.url);
const { NativeRuntime } = require(
  join(root, 'packages/octocode-native/js/runtime.cjs')
);
const credential = (hostname, token = 'synthetic-home-token') => ({
  hostname,
  username: 'home-fixture-user',
  gitProtocol: 'https',
  token: {
    token,
    tokenType: 'oauth',
    refreshToken: 'synthetic-refresh',
    expiresAt: '2099-01-01T00:00:00Z',
  },
  createdAt: '2026-01-01T00:00:00Z',
  updatedAt: '2026-01-01T00:00:00Z',
});
const run = (args, env, allowFailure = false) =>
  new Promise((done, reject) => {
    const child = spawn(process.execPath, args, { env, timeout: 60000 });
    let out = '',
      err = '';
    child.stdout.on('data', b => {
      out += b;
    });
    child.stderr.on('data', b => {
      err += b;
    });
    child.on('error', reject);
    child.on('close', code => {
      if (allowFailure) done({ code, out, err });
      else if (code === 0) done(out);
      else reject(new Error(`exit ${code}: ${err} ${out}`));
    });
  });

// Node writes and reads the main home format (AES-256-GCM, 16-byte IV,
// `iv:tag:ciphertext` hex) independently of native.
const seedHome = async (home, credentials) => {
  const key = randomBytes(32);
  await writeFile(join(home, '.key'), key.toString('hex'), { mode: 0o600 });
  const iv = randomBytes(16);
  const cipher = createCipheriv('aes-256-gcm', key, iv);
  const document = {
    version: 1,
    credentials: Object.fromEntries(credentials.map(c => [c.hostname, c])),
  };
  const ciphertext = Buffer.concat([
    cipher.update(JSON.stringify(document)),
    cipher.final(),
  ]);
  await writeFile(
    join(home, 'credentials.json'),
    [iv, cipher.getAuthTag(), ciphertext].map(b => b.toString('hex')).join(':'),
    { mode: 0o600 }
  );
};
const readHome = async home => {
  const key = Buffer.from(
    (await readFile(join(home, '.key'), 'utf8')).trim(),
    'hex'
  );
  const encoded = await readFile(join(home, 'credentials.json'), 'utf8');
  const [iv, tag, ciphertext] = encoded
    .split(':')
    .map(s => Buffer.from(s, 'hex'));
  assert.equal(iv.length, 16);
  assert.equal(tag.length, 16);
  const decipher = createDecipheriv('aes-256-gcm', key, iv);
  decipher.setAuthTag(tag);
  assert.ok(!encoded.includes('synthetic-'));
  return JSON.parse(
    Buffer.concat([decipher.update(ciphertext), decipher.final()])
  );
};

const dir = await mkdtemp(join(tmpdir(), 'octocode-home-auth-'));
const home = join(dir, 'home');
await mkdir(home);
await mkdir(join(dir, 'bin'));
await writeFile(
  join(dir, 'bin/gh'),
  '#!/bin/sh\nprintf x >> "$OCTOCODE_HOME/gh-calls"\nprintf synthetic-gh-token\n',
  { mode: 0o700 }
);
let expected = 'synthetic-home-token',
  requests = 0,
  failures = 0;
const server = createServer((req, res) => {
  requests++;
  if (req.headers.authorization !== `Bearer ${expected}`) {
    failures++;
    res.writeHead(401);
    res.end('{}');
    return;
  }
  const path = new URL(req.url, 'http://localhost').pathname;
  const body = path.endsWith('/user')
    ? { login: 'http-fixture-user' }
    : path.endsWith('/commits/main')
      ? { sha: 'a'.repeat(40) }
      : path.endsWith('/commits')
        ? []
        : {
            type: 'file',
            encoding: 'base64',
            content: Buffer.from('home store works\n').toString('base64'),
            size: 17,
            sha: 'b'.repeat(40),
            path: 'readme.txt',
          };
  res.setHeader('content-type', 'application/json');
  res.end(JSON.stringify(body));
});
await new Promise(done => server.listen(0, '127.0.0.1', done));
const env = {
  HOME: dir,
  OCTOCODE_HOME: home,
  PATH: `${dir}/bin:/usr/bin:/bin`,
  WORKSPACE_ROOT: dir,
  ALLOWED_PATHS: dir,
  OCTOCODE_ENABLE_STATS: 'false',
  MAX_RETRIES: '0',
  REQUEST_TIMEOUT: '10000',
  GITHUB_API_URL: `http://127.0.0.1:${server.address().port}/api/v3`,
};
const cli = (...args) =>
  run([join(root, 'packages/octocode/out/octocode.js'), ...args], env);
const query = {
  reasoning: 'Verify encrypted home auth through packaged interfaces.',
  owner: 'a',
  repo: 'b',
  path: 'readme.txt',
  ref: 'main',
  forceRefresh: true,
};
// Logout exercises a unique fictional host, never a real user's OS entry.
const logoutHost = `octocode-fixture-${process.pid}.invalid`;
const connectMcp = async () => {
  const mcpClient = new Client({ name: 'home-auth-test', version: '1.0.0' });
  await mcpClient.connect(
    new StdioClientTransport({
      command: process.execPath,
      args: [join(root, 'packages/octocode-mcp/dist/index.js')],
      env,
      stderr: 'pipe',
    })
  );
  return mcpClient;
};
let runtime, client;
try {
  await seedHome(home, [credential('127.0.0.1'), credential(logoutHost)]);
  // A fresh runtime loads the Node-written home.
  runtime = new NativeRuntime({ surface: 'mcp', env });
  const status = JSON.parse(await cli('auth', 'status', '--json'));
  assert.equal(status.tokenSource, 'octocode-storage');
  // GitHub accepted the home token: the verified login is the fixture's.
  assert.equal(status.verification, 'verified');
  assert.equal(status.username, 'http-fixture-user');
  assert.ok(!JSON.stringify(status).includes('synthetic-'));
  const addon = await runtime.executeMcp('home-addon', 'ghGetFileContent', {
    queries: [query],
  });
  assert.ok(JSON.stringify(addon).includes('home store works'));
  assert.ok(!JSON.stringify(addon).includes('synthetic-'));
  client = await connectMcp();
  const mcp = await client.callTool({
    name: 'ghGetFileContent',
    arguments: { queries: [query] },
  });
  assert.ok(JSON.stringify(mcp).includes('home store works'));
  assert.ok(!JSON.stringify(mcp).includes('synthetic-'));
  await assert.rejects(readFile(join(home, 'gh-calls')), { code: 'ENOENT' });
  // Another OCTOCODE_HOME never sees this home's credentials.
  const isolated = await run(
    [
      join(root, 'packages/octocode/out/octocode.js'),
      'auth',
      'status',
      '--json',
    ],
    { ...env, OCTOCODE_HOME: join(dir, 'other'), PATH: '/usr/bin:/bin' },
    true
  );
  assert.notEqual(JSON.parse(isolated.out).tokenSource, 'octocode-storage');
  const logout = await run(
    [join(root, 'packages/octocode/out/octocode.js'), 'auth', 'logout'],
    { ...env, GITHUB_API_URL: `https://${logoutHost}/api/v3` },
    true
  );
  // A headless host may deny OS-keychain access. That must remain an explicit
  // partial failure, while the home credential is gone and other hosts survive.
  if (logout.code !== 0) {
    assert.equal(logout.code, 1);
    assert.match(logout.err, /secure credential store operation failed/);
  }
  // Native rewrote the home: Node decrypts the main format it wrote.
  const remaining = await readHome(home);
  assert.equal(remaining.version, 1);
  assert.equal(remaining.credentials[logoutHost], undefined);
  assert.deepEqual(remaining.credentials['127.0.0.1'], credential('127.0.0.1'));
  if (process.platform !== 'win32') {
    for (const name of ['credentials.json', '.key', '.credentials.lock']) {
      assert.equal((await stat(join(home, name))).mode & 0o777, 0o600);
    }
  }
  // Corruption fails closed for the home, yet discovery in a fresh process
  // can still use gh. (A running process keeps its pinned selection.)
  await writeFile(join(home, 'credentials.json'), 'corrupt');
  await client.close();
  await runtime.close();
  runtime = new NativeRuntime({ surface: 'mcp', env });
  client = await connectMcp();
  expected = 'synthetic-gh-token';
  const fallback = JSON.parse(await cli('auth', 'status', '--json'));
  assert.equal(fallback.tokenSource, 'gh-cli');
  assert.equal(fallback.username, 'http-fixture-user');
  for (const result of [
    await runtime.executeMcp('gh-fallback-addon', 'ghGetFileContent', {
      queries: [query],
    }),
    await client.callTool({
      name: 'ghGetFileContent',
      arguments: { queries: [query] },
    }),
  ]) {
    assert.ok(JSON.stringify(result).includes('home store works'));
    assert.ok(!JSON.stringify(result).includes('synthetic-'));
  }
  assert.equal(
    await readFile(join(home, 'credentials.json'), 'utf8'),
    'corrupt'
  );
  assert.equal(failures, 0);
  console.log(
    JSON.stringify({
      nodeFormatRead: 'passed',
      cli: 'passed',
      addon: 'passed',
      stdioMcp: 'passed',
      homeIsolation: 'passed',
      homeLogout: 'passed',
      mainFormatWrite: 'passed',
      osLogout:
        logout.code === 0
          ? 'passed'
          : 'unavailable; partial failure correctly reported',
      corruptHomeGhFallback: 'passed',
      requests,
    })
  );
} finally {
  await client?.close();
  await runtime?.close();
  await new Promise(done => server.close(done));
  await rm(dir, { recursive: true, force: true });
}
