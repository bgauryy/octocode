// Run after native, CLI, and MCP debug builds. All secrets and homes are fixtures.
import assert from 'node:assert/strict';
import { createDecipheriv } from 'node:crypto';
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

if (process.argv[2] === '--writer') {
  const runtime = new NativeRuntime({ surface: 'mcp', env: process.env });
  try {
    runtime.storeCredentials(credential(process.argv[3]));
  } finally {
    await runtime.close();
  }
} else {
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
    branch: 'main',
    forceRefresh: true,
  };
  let runtime, client;
  try {
    runtime = new NativeRuntime({ surface: 'mcp', env });
    runtime.storeCredentials(credential('127.0.0.1'));
    // Independent Node crypto decryption verifies Rust writes the main format.
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
    const document = JSON.parse(
      Buffer.concat([decipher.update(ciphertext), decipher.final()])
    );
    assert.equal(document.version, 1);
    assert.deepEqual(
      document.credentials['127.0.0.1'],
      credential('127.0.0.1')
    );
    assert.ok(!encoded.includes('synthetic-'));
    if (process.platform !== 'win32') {
      for (const name of ['credentials.json', '.key', '.credentials.lock']) {
        assert.equal((await stat(join(home, name))).mode & 0o777, 0o600);
      }
    }
    await runtime.close();
    runtime = new NativeRuntime({ surface: 'mcp', env });
    assert.equal(
      runtime.getCredentials('127.0.0.1').username,
      'home-fixture-user'
    );
    const refreshed = await runtime.getTokenWithRefresh('127.0.0.1');
    assert.equal(refreshed.source, 'stored');
    assert.equal(refreshed.token, expected);
    const status = JSON.parse(await cli('auth', 'status', '--json'));
    assert.equal(status.tokenSource, 'octocode-storage');
    assert.equal(status.username, 'home-fixture-user');
    assert.ok(!JSON.stringify(status).includes('synthetic-'));
    const addon = await runtime.executeMcp('home-addon', 'ghGetFileContent', {
      queries: [query],
    });
    assert.ok(JSON.stringify(addon).includes('home store works'));
    assert.ok(!JSON.stringify(addon).includes('synthetic-'));
    client = new Client({ name: 'home-auth-test', version: '1.0.0' });
    await client.connect(
      new StdioClientTransport({
        command: process.execPath,
        args: [join(root, 'packages/octocode-mcp/dist/index.js')],
        env,
        stderr: 'pipe',
      })
    );
    const mcp = await client.callTool({
      name: 'ghGetFileContent',
      arguments: { queries: [query] },
    });
    assert.ok(JSON.stringify(mcp).includes('home store works'));
    assert.ok(!JSON.stringify(mcp).includes('synthetic-'));
    await assert.rejects(readFile(join(home, 'gh-calls')), { code: 'ENOENT' });
    // Separate OS processes must not lose one another's credential rows.
    await Promise.all(
      Array.from({ length: 4 }, (_, i) =>
        run(
          [fileURLToPath(import.meta.url), '--writer', `writer-${i}.invalid`],
          env
        )
      )
    );
    for (let i = 0; i < 4; i++)
      assert.equal(
        runtime.getCredentials(`writer-${i}.invalid`).username,
        'home-fixture-user'
      );
    const otherHome = join(dir, 'other');
    const isolated = new NativeRuntime({
      surface: 'mcp',
      env: { ...env, OCTOCODE_HOME: otherHome },
    });
    try {
      isolated.storeCredentials(
        credential('127.0.0.1', 'synthetic-isolated-token')
      );
      assert.equal(
        isolated.getCredentials('127.0.0.1').token.token,
        'synthetic-isolated-token'
      );
      assert.equal(
        runtime.getCredentials('127.0.0.1').token.token,
        'synthetic-home-token'
      );
    } finally {
      await isolated.close();
    }
    // Logout exercises a unique fictional host, never a real user's OS entry.
    const logoutHost = `octocode-fixture-${process.pid}.invalid`;
    runtime.storeCredentials(credential(logoutHost));
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
    const [logoutIv, logoutTag, logoutCiphertext] = (
      await readFile(join(home, 'credentials.json'), 'utf8')
    )
      .split(':')
      .map(s => Buffer.from(s, 'hex'));
    const afterLogout = createDecipheriv('aes-256-gcm', key, logoutIv);
    afterLogout.setAuthTag(logoutTag);
    const remaining = JSON.parse(
      Buffer.concat([afterLogout.update(logoutCiphertext), afterLogout.final()])
    );
    assert.equal(remaining.credentials[logoutHost], undefined);
    assert.equal(
      runtime.getCredentials('127.0.0.1').username,
      'home-fixture-user'
    );
    // Corruption fails closed for explicit reads, yet discovery can still use gh.
    await writeFile(join(home, 'credentials.json'), 'corrupt');
    assert.throws(() => runtime.getCredentials('127.0.0.1'));
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
        mainFormatWrite: 'passed',
        homePersistence: 'passed',
        cli: 'passed',
        addon: 'passed',
        stdioMcp: 'passed',
        concurrentProcesses: 'passed',
        homeIsolation: 'passed',
        homeLogout: 'passed',
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
}
